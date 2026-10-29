//! Node classification service

use chrono::{DateTime, Duration, Timelike, Utc};
use regex::Regex;
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

use crate::models::{
    ClassificationExplain, ClassificationPluginFact, ClassificationResult, ClassificationRule,
    ExprTrace, Fact, GroupExplanation, GroupMatch, GroupOutcome, Inheritance, MatchMode, MatchType,
    MergeStrategy, NodeGroup, Origin, Provenance, RuleEvaluation, RuleExplanation, RuleMatchType,
    RuleOperator,
};
use crate::models::{DynamicMembers, GroupKind};
use crate::services::classification_expr::{ClassificationExtensions, EvalContext, Expr};
use std::sync::Arc;

/// Classification service for matching nodes to groups
pub struct ClassificationService {
    groups: Vec<NodeGroup>,
    /// Parsed expressions; an invalid stored expression never matches.
    expressions: HashMap<Uuid, Result<Expr, String>>,
    /// Supplied to expressions and group windows, minute granularity.
    now: DateTime<Utc>,
    /// Functions, operators and merge strategies from plugins (M27).
    ext: Option<Arc<dyn ClassificationExtensions>>,
    /// Materialized members of the dynamic groups (M27).
    members: HashMap<Uuid, DynamicMembers>,
}

/// Result of evaluating one group for one node.
struct GroupEval {
    outcome: GroupOutcome,
    match_type: MatchType,
    matched_rules: Vec<Uuid>,
    reason: String,
    rules: Vec<RuleExplanation>,
    expression: Option<ExprTrace>,
}

/// A group's contribution to the merged classes and variables.
struct Applied<'a> {
    group: &'a NodeGroup,
    classes: serde_json::Value,
    variables: serde_json::Value,
}

fn truncate_to_minute(t: DateTime<Utc>) -> DateTime<Utc> {
    t.with_second(0)
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(t)
}

impl ClassificationService {
    /// Create a new classification service
    pub fn new(groups: Vec<NodeGroup>) -> Self {
        let expressions = groups
            .iter()
            .filter_map(|g| {
                let expr = g.expression.as_ref()?;
                Some((g.id, Expr::parse(expr)))
            })
            .collect();
        Self {
            groups,
            expressions,
            now: truncate_to_minute(Utc::now()),
            ext: None,
            members: HashMap::new(),
        }
    }

    /// Plugin functions, operators and merge strategies (M27).
    pub fn with_extensions(mut self, ext: Arc<dyn ClassificationExtensions>) -> Self {
        self.ext = Some(ext);
        self
    }

    /// Materialized members of dynamic groups (M27), already filtered by the
    /// stale policy of each group.
    pub fn with_members(mut self, members: HashMap<Uuid, DynamicMembers>) -> Self {
        self.members = members;
        self
    }

    fn with_shared(
        mut self,
        ext: Option<Arc<dyn ClassificationExtensions>>,
        members: HashMap<Uuid, DynamicMembers>,
    ) -> Self {
        self.ext = ext;
        self.members = members;
        self
    }

    /// Fix the instant used for expressions and group windows (tests, previews).
    pub fn with_now(mut self, now: DateTime<Utc>) -> Self {
        self.now = truncate_to_minute(now);
        self
    }

    /// Classify a node based on its facts (supports hierarchical groups and inheritance)
    ///
    /// This method classifies against groups from a single organization.
    /// For multi-organization classification with conflict detection, use `classify_across_organizations`.
    pub fn classify(&self, certname: &str, facts: &serde_json::Value) -> ClassificationResult {
        self.classify_internal(certname, facts, None, false).0
    }

    /// Like [`classify`](Self::classify), plus why each group did or did not
    /// match and where every class, variable and the environment came from.
    pub fn classify_explained(
        &self,
        certname: &str,
        facts: &serde_json::Value,
    ) -> (ClassificationResult, ClassificationExplain) {
        let (result, explain) = self.classify_internal(certname, facts, None, true);
        (result, explain.expect("explain requested"))
    }

    /// Classify a node against groups from all organizations and detect conflicts
    ///
    /// This method:
    /// 1. Groups the input groups by organization
    /// 2. Classifies the node against each organization's groups
    /// 3. Detects if the node matches groups from multiple organizations (conflict)
    /// 4. Returns the classification from the matching organization, or an error if conflicted
    /// 5. If no groups match, uses the default organization
    pub fn classify_across_organizations(
        &self,
        certname: &str,
        facts: &serde_json::Value,
        default_org_id: Uuid,
    ) -> ClassificationResult {
        // Group all groups by organization
        let mut org_groups: HashMap<Uuid, Vec<&NodeGroup>> = HashMap::new();
        for group in &self.groups {
            org_groups
                .entry(group.organization_id)
                .or_default()
                .push(group);
        }

        // Classify against each organization and track which ones have matches
        let mut org_results: Vec<(Uuid, ClassificationResult)> = Vec::new();

        for (org_id, groups) in &org_groups {
            let org_service =
                ClassificationService::new(groups.iter().map(|g| (*g).clone()).collect())
                    .with_now(self.now)
                    .with_shared(self.ext.clone(), self.members.clone());
            let result = org_service
                .classify_internal(certname, facts, Some(*org_id), false)
                .0;

            // Check if this organization had any non-inherited matches
            // (inherited matches don't count as the node being "in" that org)
            let has_direct_matches = result
                .groups
                .iter()
                .any(|g| g.match_type == MatchType::Rules || g.match_type == MatchType::Pinned);

            if has_direct_matches {
                org_results.push((*org_id, result));
            }
        }

        // Check for conflicts (node matches in multiple organizations)
        if org_results.len() > 1 {
            let org_names: Vec<String> = org_results.iter().map(|(id, _)| id.to_string()).collect();

            // Return the first result but with a conflict error
            let mut result = org_results.remove(0).1;
            result.conflict_error = Some(format!(
                "Node '{}' matches groups from multiple organizations: {}. \
                 Please ensure nodes are assigned to only one organization.",
                certname,
                org_names.join(", ")
            ));
            return result;
        }

        // Return the single matching org's result, or empty result with default org
        if let Some((org_id, mut result)) = org_results.pop() {
            result.organization_id = Some(org_id);
            result
        } else {
            // No matches - return empty classification with default org
            ClassificationResult {
                certname: certname.to_string(),
                organization_id: Some(default_org_id),
                groups: vec![],
                classes: serde_json::json!({}),
                variables: serde_json::json!({}),
                environment: None,
                conflict_error: None,
            }
        }
    }

    /// True unless the group is outside its `active_from` / `expires_at` window.
    fn is_active(&self, group: &NodeGroup) -> bool {
        group
            .behavior
            .active_from
            .is_none_or(|from| self.now >= from)
            && group.behavior.expires_at.is_none_or(|to| self.now < to)
    }

    /// Evaluate one group for a node, without looking at its exclusivity.
    fn evaluate_group(
        &self,
        group: &NodeGroup,
        facts: &serde_json::Value,
        parent_matched: bool,
        node_environment: Option<&str>,
        certname: &str,
        explain: bool,
    ) -> GroupEval {
        let eval = |outcome: GroupOutcome, reason: String| GroupEval {
            outcome,
            match_type: MatchType::Rules,
            matched_rules: vec![],
            reason,
            rules: vec![],
            expression: None,
        };

        if !self.is_active(group) {
            return eval(
                GroupOutcome::Inactive,
                "outside the group's active window".into(),
            );
        }
        if group.pinned_nodes.iter().any(|n| n == certname) {
            return GroupEval {
                match_type: MatchType::Pinned,
                ..eval(
                    GroupOutcome::Matched,
                    "the node is pinned to the group".into(),
                )
            };
        }

        // Dynamic groups: membership comes from the materialized provider data
        // (no I/O on this path); rules and expressions do not apply.
        if group.behavior.kind == GroupKind::Dynamic {
            let node_environment_ok = group.is_environment_group
                || group.environment.as_deref().is_none_or(|env| {
                    env == "*"
                        || env.eq_ignore_ascii_case("all")
                        || env.eq_ignore_ascii_case("any")
                        || node_environment == Some(env)
                });
            if !node_environment_ok {
                return eval(
                    GroupOutcome::EnvironmentMismatch,
                    "the node is not in the group's environment".into(),
                );
            }
            let member = self
                .members
                .get(&group.id)
                .is_some_and(|m| m.certnames.contains(certname));
            return eval(
                if member {
                    GroupOutcome::Matched
                } else {
                    GroupOutcome::NotMatched
                },
                match (member, self.members.get(&group.id).is_some_and(|m| m.stale)) {
                    (true, false) => "member of the dynamic group".to_string(),
                    (true, true) => {
                        "member of the dynamic group (provider data is stale)".to_string()
                    }
                    (false, _) => "not a member of the dynamic group".to_string(),
                },
            );
        }

        // Environment groups ASSIGN environments instead of filtering by them.
        if !group.is_environment_group {
            if let Some(env) = &group.environment {
                let any_env = env == "*"
                    || env.eq_ignore_ascii_case("all")
                    || env.eq_ignore_ascii_case("any");
                if !any_env && node_environment != Some(env.as_str()) {
                    return eval(
                        GroupOutcome::EnvironmentMismatch,
                        format!(
                            "the group requires environment '{env}', the node has {}",
                            node_environment.unwrap_or("none")
                        ),
                    );
                }
            }
        }

        let expression = self.expressions.get(&group.id);
        if group.rules.is_empty() && expression.is_none() {
            // Group has no conditions: match_all_nodes decides (inside the
            // parent's context when it has a parent, unless independent).
            let parent_ok =
                parent_matched || group.behavior.inheritance == Inheritance::Independent;
            let matched = group.match_all_nodes && (group.parent_id.is_none() || parent_ok);
            let reason = match (group.match_all_nodes, matched) {
                (true, true) => "no conditions and match_all_nodes is on".to_string(),
                (true, false) => "match_all_nodes is on but the parent did not match".to_string(),
                (false, _) => "no rules or expression and match_all_nodes is off".to_string(),
            };
            return eval(
                if matched {
                    GroupOutcome::Matched
                } else {
                    GroupOutcome::NotMatched
                },
                reason,
            );
        }

        let mut result = eval(GroupOutcome::NotMatched, String::new());
        let mut parts: Vec<String> = Vec::new();
        let mut matched = true;

        if !group.rules.is_empty() {
            let evaluations = self.evaluate_rules(&group.rules, facts);
            result.matched_rules = evaluations
                .iter()
                .filter(|e| e.matched)
                .map(|e| e.rule_id)
                .collect();
            let count = evaluations.iter().filter(|e| e.matched).count();
            let mode = group
                .behavior
                .match_mode
                .clone()
                .unwrap_or(match group.rule_match_type {
                    RuleMatchType::All => MatchMode::All,
                    RuleMatchType::Any => MatchMode::Any,
                });
            let rules_match = match &mode {
                MatchMode::All => {
                    evaluations.iter().all(|e| e.matched || e.error.is_some()) && count > 0
                }
                MatchMode::Any => count > 0,
                MatchMode::NOf { k } => count >= *k,
            };
            parts.push(format!(
                "{count} of {} rules matched (needs {})",
                group.rules.len(),
                match &mode {
                    MatchMode::All => "all".to_string(),
                    MatchMode::Any => "any".to_string(),
                    MatchMode::NOf { k } => format!("{k}"),
                }
            ));
            matched &= rules_match;
            if explain {
                result.rules = group
                    .rules
                    .iter()
                    .zip(&evaluations)
                    .map(|(rule, e)| RuleExplanation {
                        rule_id: rule.id,
                        fact_path: rule.fact_path.clone(),
                        operator: rule.operator,
                        expected: rule.value.clone(),
                        actual: e.fact_value.clone(),
                        matched: e.matched,
                    })
                    .collect();
            }
        }

        match expression {
            Some(Ok(expr)) => {
                let ctx = EvalContext {
                    facts,
                    now: self.now,
                    ext: self.ext.as_deref(),
                    unavailable: Default::default(),
                };
                let expr_matched = if explain {
                    let trace = expr.trace(&ctx);
                    let m = trace.matched;
                    result.expression = Some(trace);
                    m
                } else {
                    expr.matches(&ctx)
                };
                parts.push(format!(
                    "expression {}",
                    if expr_matched {
                        "matched"
                    } else {
                        "did not match"
                    }
                ));
                matched &= expr_matched;
            }
            Some(Err(e)) => {
                parts.push(format!("expression is invalid ({e})"));
                matched = false;
            }
            None => {}
        }

        result.outcome = if matched {
            GroupOutcome::Matched
        } else {
            GroupOutcome::NotMatched
        };
        result.reason = parts.join("; ");
        result
    }

    /// For a group in an `exclusive_set`: the sibling of the same set that
    /// stays (pinned first, then priority, then name). `None` when the group
    /// is not in a set.
    fn exclusion_winner(
        &self,
        group: &NodeGroup,
        facts: &serde_json::Value,
        node_environment: Option<&str>,
        certname: &str,
    ) -> Option<(Uuid, String)> {
        let set = group.behavior.exclusive_set.as_ref()?;
        self.groups
            .iter()
            .filter(|g| {
                g.parent_id == group.parent_id && g.behavior.exclusive_set.as_ref() == Some(set)
            })
            .filter_map(|g| {
                let e = self.evaluate_group(g, facts, true, node_environment, certname, false);
                (e.outcome == GroupOutcome::Matched).then_some((
                    e.match_type == MatchType::Pinned,
                    g.priority,
                    g,
                ))
            })
            .max_by(|(ap, apr, a), (bp, bpr, b)| {
                ap.cmp(bp).then(apr.cmp(bpr)).then(b.name.cmp(&a.name))
            })
            .map(|(_, _, g)| (g.id, g.name.clone()))
    }

    /// Internal classification method that handles the actual matching logic
    fn classify_internal(
        &self,
        certname: &str,
        facts: &serde_json::Value,
        organization_id: Option<Uuid>,
        explain: bool,
    ) -> (ClassificationResult, Option<ClassificationExplain>) {
        let started = std::time::Instant::now();
        if let Some(ext) = &self.ext {
            ext.begin_node();
        }
        let mut matched_groups: Vec<GroupMatch> = vec![];
        let mut environment: Option<String> = None;
        let mut environment_origin: Option<Origin> = None;
        let mut explanations: Vec<GroupExplanation> = vec![];

        // Extract node's environment from facts (catalog_environment)
        let node_environment = get_fact_value(facts, "catalog_environment")
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        // Index groups by id and build parent -> children map; siblings are
        // ordered by priority (stable, so equal priorities keep their order
        // and the higher priority merges last and wins)
        let mut children_map: HashMap<Option<Uuid>, Vec<&NodeGroup>> = HashMap::new();
        let mut group_index: HashMap<Uuid, &NodeGroup> = HashMap::new();
        for group in &self.groups {
            children_map.entry(group.parent_id).or_default().push(group);
            group_index.insert(group.id, group);
        }
        for siblings in children_map.values_mut() {
            siblings.sort_by_key(|g| g.priority);
        }

        // Get all ancestors (parent chain) from root to group
        let get_ancestor_chain = |group_id: Uuid| -> Vec<&NodeGroup> {
            let mut chain = vec![];
            let mut current_id = group_id;
            let mut seen = HashSet::new();
            while let Some(group) = group_index.get(&current_id) {
                if !seen.insert(group.id) {
                    break;
                }
                chain.push(*group);
                if let Some(parent_id) = group.parent_id {
                    current_id = parent_id;
                } else {
                    break;
                }
            }
            chain.reverse();
            chain
        };

        // The environment of a matched group: the closest group of its chain
        // that was matched and sets one.
        let environment_of = |group_id: Uuid, matched: &[GroupMatch]| -> Option<(String, Origin)> {
            get_ancestor_chain(group_id)
                .into_iter()
                .rev()
                .filter(|a| a.id == group_id || matched.iter().any(|m| m.id == a.id))
                .find_map(|a| match &a.environment {
                    Some(env)
                        if env == "*"
                            || env.to_lowercase() == "all"
                            || env.to_lowercase() == "any" =>
                    {
                        None
                    }
                    Some(env) => Some((
                        env.clone(),
                        Origin {
                            group_id: a.id,
                            group_name: a.name.clone(),
                        },
                    )),
                    None => None,
                })
        };

        // Groups where this node is pinned match even if their parent chain
        // does not match through normal rule evaluation
        let mut pinned_group_ids: Vec<Uuid> = vec![];
        for g in &self.groups {
            if !g.pinned_nodes.iter().any(|n| n == certname) || !self.is_active(g) {
                continue;
            }
            if let Some((winner, _)) =
                self.exclusion_winner(g, facts, node_environment.as_deref(), certname)
            {
                if winner != g.id {
                    continue;
                }
            }
            pinned_group_ids.push(g.id);
        }

        // Groups already handled (pinned chains) and groups already visited
        let mut processed_groups: HashSet<Uuid> = HashSet::new();
        let mut visited: HashSet<Uuid> = HashSet::new();

        for pinned_id in &pinned_group_ids {
            let Some(pinned_group) = group_index.get(pinned_id) else {
                continue;
            };
            let ancestor_chain = get_ancestor_chain(*pinned_id);

            // Add the pinned group plus every ancestor as `Inherited`: pinning
            // to a leaf makes the node a logical member of every group above
            // it, otherwise ancestor-level counts silently omit these nodes.
            for ancestor in &ancestor_chain {
                if matched_groups.iter().any(|g| g.id == ancestor.id) {
                    continue;
                }
                let is_pinned = ancestor.id == pinned_group.id;
                let match_type = if is_pinned {
                    MatchType::Pinned
                } else {
                    MatchType::Inherited
                };
                matched_groups.push(GroupMatch {
                    id: ancestor.id,
                    name: ancestor.name.clone(),
                    match_type,
                    matched_rules: vec![],
                });
                if explain {
                    explanations.push(GroupExplanation {
                        id: ancestor.id,
                        name: ancestor.name.clone(),
                        parent_id: ancestor.parent_id,
                        priority: ancestor.priority,
                        outcome: GroupOutcome::Matched,
                        match_type: Some(match_type),
                        reason: if is_pinned {
                            "the node is pinned to the group".into()
                        } else {
                            format!("ancestor of the pinned group '{}'", pinned_group.name)
                        },
                        rules: vec![],
                        expression: None,
                    });
                }
                processed_groups.insert(ancestor.id);
                visited.insert(ancestor.id);
            }

            // Leaf (pinned) group's environment takes precedence
            if let Some((env, origin)) = environment_of(*pinned_id, &matched_groups) {
                environment = Some(env);
                environment_origin = Some(origin);
            }
        }

        // Independent groups are evaluated after the normal walk, when their
        // ancestors' outcome is known
        let mut independents: Vec<&NodeGroup> = self
            .groups
            .iter()
            .filter(|g| g.parent_id.is_some() && g.behavior.inheritance == Inheritance::Independent)
            .collect();
        independents.sort_by_key(|g| (get_ancestor_chain(g.id).len(), g.priority));
        let mut independents = independents.into_iter();

        // BFS from root groups; children are only considered if parent matched
        let mut queue: VecDeque<(&NodeGroup, bool)> = VecDeque::new();
        if let Some(roots) = children_map.get(&None) {
            for g in roots {
                queue.push_back((*g, false));
            }
        }

        loop {
            let Some((group, parent_matched)) = queue.pop_front() else {
                match independents.next() {
                    Some(g) => {
                        if !visited.contains(&g.id) {
                            queue.push_back((g, false));
                        }
                        continue;
                    }
                    None => break,
                }
            };

            // Skip groups already processed via pinning, but still walk on
            // to their children
            if processed_groups.contains(&group.id) {
                if let Some(children) = children_map.get(&Some(group.id)) {
                    for child in children {
                        if !processed_groups.contains(&child.id)
                            && child.behavior.inheritance != Inheritance::Independent
                        {
                            queue.push_back((*child, true));
                        }
                    }
                }
                continue;
            }
            if !visited.insert(group.id) {
                continue;
            }

            let mut eval = self.evaluate_group(
                group,
                facts,
                parent_matched,
                node_environment.as_deref(),
                certname,
                explain,
            );

            if eval.outcome == GroupOutcome::Matched && eval.match_type != MatchType::Pinned {
                if let Some((winner, winner_name)) =
                    self.exclusion_winner(group, facts, node_environment.as_deref(), certname)
                {
                    if winner != group.id {
                        eval.outcome = GroupOutcome::Excluded;
                        eval.reason = format!(
                            "excluded by exclusive_set '{}': '{}' has precedence",
                            group.behavior.exclusive_set.as_deref().unwrap_or_default(),
                            winner_name
                        );
                    }
                }
            }

            if explain {
                explanations.push(GroupExplanation {
                    id: group.id,
                    name: group.name.clone(),
                    parent_id: group.parent_id,
                    priority: group.priority,
                    outcome: eval.outcome,
                    match_type: (eval.outcome == GroupOutcome::Matched).then_some(eval.match_type),
                    reason: eval.reason.clone(),
                    rules: eval.rules.clone(),
                    expression: eval.expression.clone(),
                });
            }

            if eval.outcome != GroupOutcome::Matched {
                // Children of a group that did not match are not evaluated
                continue;
            }

            matched_groups.push(GroupMatch {
                id: group.id,
                name: group.name.clone(),
                match_type: eval.match_type,
                matched_rules: eval.matched_rules,
            });
            if let Some((env, origin)) = environment_of(group.id, &matched_groups) {
                environment = Some(env);
                environment_origin = Some(origin);
            }

            if let Some(children) = children_map.get(&Some(group.id)) {
                for child in children {
                    if child.behavior.inheritance != Inheritance::Independent {
                        queue.push_back((*child, true));
                    }
                }
            }
        }

        let (classes, variables, mut provenance, merge_errors) =
            self.merge_matched(&matched_groups, &group_index, explain);
        provenance.environment = environment_origin;

        let explain = explain.then(|| {
            let evaluated: HashSet<Uuid> = explanations.iter().map(|e| e.id).collect();
            for group in &self.groups {
                if evaluated.contains(&group.id) {
                    continue;
                }
                let parent = group
                    .parent_id
                    .and_then(|p| group_index.get(&p))
                    .map(|p| p.name.as_str());
                explanations.push(GroupExplanation {
                    id: group.id,
                    name: group.name.clone(),
                    parent_id: group.parent_id,
                    priority: group.priority,
                    outcome: GroupOutcome::NotEvaluated,
                    match_type: None,
                    reason: match parent {
                        Some(p) => format!("the parent group '{p}' did not match"),
                        None => "not reachable".to_string(),
                    },
                    rules: vec![],
                    expression: None,
                });
            }
            ClassificationExplain {
                evaluated_at: self.now,
                elapsed_us: started.elapsed().as_micros() as u64,
                groups: explanations,
                provenance,
                merge_errors: merge_errors.clone(),
                degraded: self.ext.as_ref().map(|e| e.degraded()).unwrap_or_default(),
            }
        });

        let conflict_error = (!merge_errors.is_empty())
            .then(|| format!("Merge conflict: {}", merge_errors.join("; ")));

        (
            ClassificationResult {
                certname: certname.to_string(),
                organization_id,
                groups: matched_groups,
                classes,
                variables,
                environment,
                conflict_error,
            },
            explain,
        )
    }

    /// Merge the classes and variables of the matched groups, in match order
    /// (ancestors first), applying each group's inheritance and merge policy.
    fn merge_matched<'a>(
        &self,
        matched: &[GroupMatch],
        group_index: &HashMap<Uuid, &'a NodeGroup>,
        explain: bool,
    ) -> (
        serde_json::Value,
        serde_json::Value,
        Provenance,
        Vec<String>,
    ) {
        let mut applied: Vec<Applied<'a>> = Vec::with_capacity(matched.len());
        for m in matched {
            let Some(group) = group_index.get(&m.id).copied() else {
                continue;
            };
            if !matches!(
                group.behavior.inheritance,
                Inheritance::Inherit | Inheritance::Independent
            ) {
                let mut ancestors = HashSet::new();
                let mut current = group.parent_id;
                while let Some(id) = current {
                    if !ancestors.insert(id) {
                        break;
                    }
                    current = group_index.get(&id).and_then(|g| g.parent_id);
                }
                match &group.behavior.inheritance {
                    Inheritance::Block => applied.retain(|a| !ancestors.contains(&a.group.id)),
                    Inheritance::Select { keys } => {
                        for a in applied
                            .iter_mut()
                            .filter(|a| ancestors.contains(&a.group.id))
                        {
                            a.classes = pick_keys(&a.classes, keys);
                            a.variables = pick_keys(&a.variables, keys);
                        }
                    }
                    _ => {}
                }
            }
            applied.push(Applied {
                group,
                classes: group.classes.clone(),
                variables: group.variables.clone(),
            });
        }

        let mut classes = serde_json::json!({});
        let mut variables = serde_json::json!({});
        let mut provenance = Provenance::default();
        let mut errors: Vec<String> = Vec::new();
        for a in &applied {
            let strategy = &a.group.behavior.merge;
            let class_conflicts = merge_classes_with(
                &mut classes,
                &a.classes,
                strategy,
                self.ext.as_deref(),
                &a.group.name,
                &mut errors,
            );
            let var_conflicts = merge_variables_with(
                &mut variables,
                &a.variables,
                strategy,
                self.ext.as_deref(),
                &a.group.name,
                &mut errors,
            );
            if explain {
                let origin = Origin {
                    group_id: a.group.id,
                    group_name: a.group.name.clone(),
                };
                if let Some(obj) = a.classes.as_object() {
                    for name in obj.keys().filter(|k| !class_conflicts.contains(*k)) {
                        provenance
                            .classes
                            .entry(name.clone())
                            .or_default()
                            .push(origin.clone());
                    }
                }
                if let Some(obj) = a.variables.as_object() {
                    for key in obj.keys().filter(|k| !var_conflicts.contains(*k)) {
                        provenance
                            .variables
                            .entry(key.clone())
                            .or_default()
                            .push(origin.clone());
                    }
                }
            }
        }
        (classes, variables, provenance, errors)
    }

    /// Evaluate rules against facts
    fn evaluate_rules(
        &self,
        rules: &[ClassificationRule],
        facts: &serde_json::Value,
    ) -> Vec<RuleEvaluation> {
        rules
            .iter()
            .map(|rule| self.evaluate_rule(rule, facts))
            .collect()
    }

    /// Evaluate a single rule against facts
    fn evaluate_rule(
        &self,
        rule: &ClassificationRule,
        facts: &serde_json::Value,
    ) -> RuleEvaluation {
        let fact_value = get_fact_value(facts, &rule.fact_path);
        let matched = fact_value
            .as_ref()
            .is_some_and(|value| match_value(value, &rule.operator, &rule.value));
        RuleEvaluation {
            rule_id: rule.id,
            matched,
            fact_value,
            error: None,
        }
    }
}

/// Keep only the listed keys of an object.
fn pick_keys(value: &serde_json::Value, keys: &[String]) -> serde_json::Value {
    match value.as_object() {
        Some(obj) => serde_json::Value::Object(
            obj.iter()
                .filter(|(k, _)| keys.contains(k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ),
        None => serde_json::json!({}),
    }
}

/// Union of two arrays without duplicates; objects merge recursively.
fn union_merge(target: &mut serde_json::Value, source: &serde_json::Value) {
    match (target, source) {
        (serde_json::Value::Object(t), serde_json::Value::Object(s)) => {
            for (k, v) in s {
                match t.get_mut(k) {
                    Some(existing) => union_merge(existing, v),
                    None => {
                        t.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (serde_json::Value::Array(t), serde_json::Value::Array(s)) => {
            for v in s {
                if !t.contains(v) {
                    t.push(v.clone());
                }
            }
        }
        (t, s) => *t = s.clone(),
    }
}

/// Merge `source` into `target`, keeping the first value of a key that
/// differs and reporting it. Returns whether a conflict was found.
fn merge_checked(
    path: &str,
    target: &mut serde_json::Value,
    source: &serde_json::Value,
    group: &str,
    errors: &mut Vec<String>,
) -> bool {
    match (&mut *target, source) {
        (serde_json::Value::Object(t), serde_json::Value::Object(s)) => {
            let mut conflict = false;
            for (k, v) in s {
                let child = format!("{path}.{k}");
                match t.get_mut(k) {
                    Some(existing) => conflict |= merge_checked(&child, existing, v, group, errors),
                    None => {
                        t.insert(k.clone(), v.clone());
                    }
                }
            }
            conflict
        }
        (t, s) if *t == *s => false,
        _ => {
            errors.push(format!(
                "{path} has different values; group '{group}' conflicts"
            ));
            true
        }
    }
}

/// Merge a group's classes with its strategy; returns the class names whose
/// value was rejected by `fail_on_conflict`.
fn merge_classes_with(
    target: &mut serde_json::Value,
    source: &serde_json::Value,
    strategy: &MergeStrategy,
    ext: Option<&dyn ClassificationExtensions>,
    group: &str,
    errors: &mut Vec<String>,
) -> HashSet<String> {
    let mut rejected = HashSet::new();
    match strategy {
        MergeStrategy::Deep => merge_classes(target, source),
        MergeStrategy::Replace
        | MergeStrategy::Union
        | MergeStrategy::FailOnConflict
        | MergeStrategy::Plugin(_) => {
            let (Some(t), Some(s)) = (target.as_object_mut(), source.as_object()) else {
                return rejected;
            };
            for (name, params) in s {
                match (strategy, t.get_mut(name)) {
                    (_, None) => {
                        t.insert(name.clone(), params.clone());
                    }
                    (MergeStrategy::Replace, Some(existing)) => *existing = params.clone(),
                    (MergeStrategy::Union, Some(existing)) => union_merge(existing, params),
                    (MergeStrategy::Plugin(r), Some(existing)) => {
                        match ext.map(|e| e.merge(r, name, existing, params)) {
                            Some(Ok(v)) => *existing = v,
                            // Unavailable strategy: fall back to the default
                            // deep merge of this key (the provider records why)
                            _ => deep_merge(existing, params),
                        }
                    }
                    (_, Some(existing)) => {
                        if merge_checked(&format!("class {name}"), existing, params, group, errors)
                        {
                            rejected.insert(name.clone());
                        }
                    }
                }
            }
        }
    }
    rejected
}

/// Merge a group's variables with its strategy; returns the keys whose value
/// was rejected by `fail_on_conflict`.
fn merge_variables_with(
    target: &mut serde_json::Value,
    source: &serde_json::Value,
    strategy: &MergeStrategy,
    ext: Option<&dyn ClassificationExtensions>,
    group: &str,
    errors: &mut Vec<String>,
) -> HashSet<String> {
    let mut rejected = HashSet::new();
    let (Some(t), Some(s)) = (target.as_object_mut(), source.as_object()) else {
        return rejected;
    };
    for (key, value) in s {
        match (strategy, t.get_mut(key)) {
            (_, None) => {
                t.insert(key.clone(), value.clone());
            }
            (MergeStrategy::Union, Some(existing)) => union_merge(existing, value),
            (MergeStrategy::Plugin(r), Some(existing)) => {
                match ext.map(|e| e.merge(r, key, existing, value)) {
                    Some(Ok(v)) => *existing = v,
                    _ => *existing = value.clone(),
                }
            }
            (MergeStrategy::FailOnConflict, Some(existing)) => {
                if merge_checked(&format!("variable {key}"), existing, value, group, errors) {
                    rejected.insert(key.clone());
                }
            }
            (_, Some(existing)) => *existing = value.clone(),
        }
    }
    rejected
}

/// Build a classification fact payload from PuppetDB facts.
///
/// PuppetDB returns structured facts as dotted paths such as `os.family`.
/// Classification walks fact values segment-by-segment, so those paths need to
/// be reconstructed into nested JSON before evaluation.
pub fn build_classification_facts(
    facts: Vec<Fact>,
    certname: &str,
    catalog_environment: Option<&str>,
) -> serde_json::Value {
    build_classification_facts_with_plugins(facts, certname, catalog_environment, &[], Utc::now())
}

/// Same as [`build_classification_facts`], plus the plugin facts an
/// administrator enabled for rules (M25).
///
/// A plugin fact never overwrites what the agent reported: it is skipped when
/// PuppetDB has a fact with the same name or a top-level fact named like its
/// namespace. A value older than its `max_stale_s` at `now` counts as missing,
/// so a rule on it does not match (as for any absent fact).
pub fn build_classification_facts_with_plugins(
    facts: Vec<Fact>,
    certname: &str,
    catalog_environment: Option<&str>,
    plugin_facts: &[ClassificationPluginFact],
    now: DateTime<Utc>,
) -> serde_json::Value {
    let mut root = serde_json::Map::new();

    let mut agent_names: HashSet<String> = HashSet::new();
    for fact in facts {
        if !plugin_facts.is_empty() {
            agent_names.insert(fact.name.clone());
            if let Some((top, _)) = fact.name.split_once('.') {
                agent_names.insert(top.to_string());
            }
        }
        insert_fact_value(&mut root, &fact.name, fact.value);
    }

    for pf in plugin_facts {
        let namespace = pf.name.split('.').next().unwrap_or_default();
        if agent_names.contains(&pf.name) || agent_names.contains(namespace) {
            continue;
        }
        let max_stale = Duration::seconds(pf.max_stale_s.min(i64::MAX as u64) as i64);
        if now.signed_duration_since(pf.computed_at) > max_stale {
            continue;
        }
        insert_fact_value(&mut root, &pf.name, pf.value.clone());
    }

    root.insert(
        "clientcert".to_string(),
        serde_json::Value::String(certname.to_string()),
    );
    root.insert(
        "certname".to_string(),
        serde_json::Value::String(certname.to_string()),
    );

    if let Some(environment) = catalog_environment {
        root.insert(
            "catalog_environment".to_string(),
            serde_json::Value::String(environment.to_string()),
        );
    }

    serde_json::Value::Object(root)
}

/// Get a fact value by path (e.g., "os.family" -> facts["os"]["family"])
fn get_fact_value(facts: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = facts;

    for part in parts {
        {
            let v = current.get(part)?;
            current = v
        }
    }

    Some(current.clone())
}

fn insert_fact_value(
    current: &mut serde_json::Map<String, serde_json::Value>,
    path: &str,
    value: serde_json::Value,
) {
    let mut parts = path.split('.').peekable();
    let mut node = current;

    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            node.insert(part.to_string(), value);
            return;
        }

        let entry = node
            .entry(part.to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

        if !entry.is_object() {
            *entry = serde_json::Value::Object(serde_json::Map::new());
        }

        node = entry.as_object_mut().expect("object inserted above");
    }
}

/// Match a fact value against a rule value
fn match_value(
    fact_value: &serde_json::Value,
    operator: &RuleOperator,
    rule_value: &serde_json::Value,
) -> bool {
    match operator {
        RuleOperator::Equals => fact_value == rule_value,
        RuleOperator::NotEquals => fact_value != rule_value,
        RuleOperator::Regex => {
            if let (Some(fv), Some(rv)) = (fact_value.as_str(), rule_value.as_str()) {
                Regex::new(rv).map(|re| re.is_match(fv)).unwrap_or(false)
            } else {
                false
            }
        }
        RuleOperator::NotRegex => {
            if let (Some(fv), Some(rv)) = (fact_value.as_str(), rule_value.as_str()) {
                Regex::new(rv).map(|re| !re.is_match(fv)).unwrap_or(true)
            } else {
                true
            }
        }
        RuleOperator::GreaterThan => compare_values(fact_value, rule_value) > 0,
        RuleOperator::GreaterThanOrEqual => compare_values(fact_value, rule_value) >= 0,
        RuleOperator::LessThan => compare_values(fact_value, rule_value) < 0,
        RuleOperator::LessThanOrEqual => compare_values(fact_value, rule_value) <= 0,
        RuleOperator::In => {
            if let Some(arr) = rule_value.as_array() {
                arr.contains(fact_value)
            } else {
                false
            }
        }
        RuleOperator::NotIn => {
            if let Some(arr) = rule_value.as_array() {
                !arr.contains(fact_value)
            } else {
                true
            }
        }
    }
}

/// Compare two JSON values for ordering
fn compare_values(a: &serde_json::Value, b: &serde_json::Value) -> i32 {
    match (a, b) {
        (serde_json::Value::Number(an), serde_json::Value::Number(bn)) => {
            let af = an.as_f64().unwrap_or(0.0);
            let bf = bn.as_f64().unwrap_or(0.0);
            if af > bf {
                1
            } else if af < bf {
                -1
            } else {
                0
            }
        }
        (serde_json::Value::String(as_), serde_json::Value::String(bs)) => as_.cmp(bs) as i32,
        _ => 0,
    }
}

/// Deep merge classes in Puppet Enterprise format
/// Classes are objects like: {"ntp": {"servers": ["ntp1.example.com"]}, "apache": {"port": 8080}}
/// When the same class appears in multiple groups, their parameters are deep merged
fn merge_classes(target: &mut serde_json::Value, source: &serde_json::Value) {
    if let (Some(target_obj), Some(source_obj)) = (target.as_object_mut(), source.as_object()) {
        for (class_name, class_params) in source_obj {
            if let Some(existing_params) = target_obj.get_mut(class_name) {
                // Class already exists, deep merge its parameters
                deep_merge(existing_params, class_params);
            } else {
                // New class, insert it
                target_obj.insert(class_name.clone(), class_params.clone());
            }
        }
    }
}

/// Deep merge two JSON values (recursively merges objects, overwrites other types)
fn deep_merge(target: &mut serde_json::Value, source: &serde_json::Value) {
    match (target, source) {
        (serde_json::Value::Object(target_obj), serde_json::Value::Object(source_obj)) => {
            for (key, value) in source_obj {
                if let Some(existing) = target_obj.get_mut(key) {
                    deep_merge(existing, value);
                } else {
                    target_obj.insert(key.clone(), value.clone());
                }
            }
        }
        (target, source) => {
            // For non-objects, source overwrites target
            *target = source.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_fact_value_simple() {
        let facts = serde_json::json!({
            "os": "linux"
        });

        let value = get_fact_value(&facts, "os");
        assert_eq!(value, Some(serde_json::json!("linux")));
    }

    #[test]
    fn test_get_fact_value_nested() {
        let facts = serde_json::json!({
            "os": {
                "family": "RedHat",
                "release": {
                    "major": "8"
                }
            }
        });

        let value = get_fact_value(&facts, "os.family");
        assert_eq!(value, Some(serde_json::json!("RedHat")));

        let value = get_fact_value(&facts, "os.release.major");
        assert_eq!(value, Some(serde_json::json!("8")));
    }

    #[test]
    fn test_get_fact_value_missing() {
        let facts = serde_json::json!({
            "os": "linux"
        });

        let value = get_fact_value(&facts, "missing");
        assert_eq!(value, None);
    }

    #[test]
    fn plugin_facts_never_override_agent_facts_and_stale_ones_are_missing() {
        let now = Utc::now();
        let agent = |name: &str, v: serde_json::Value| Fact {
            certname: "n1".into(),
            name: name.into(),
            value: v,
            environment: None,
        };
        let pf = |name: &str, v: serde_json::Value, age_s: i64| ClassificationPluginFact {
            plugin_id: "cmdb".into(),
            name: name.into(),
            value: v,
            computed_at: now - Duration::seconds(age_s),
            max_stale_s: 600,
        };
        let facts = vec![
            agent("os.family", serde_json::json!("RedHat")),
            agent("dup.owner", serde_json::json!("agent")),
        ];
        let plugin = vec![
            pf("acme.tier", serde_json::json!(1), 10),
            pf("acme.owner", serde_json::json!("ops"), 10),
            pf("acme.old", serde_json::json!(true), 601),
            pf("os.custom", serde_json::json!("x"), 10),
            pf("dup.owner", serde_json::json!("plugin"), 10),
        ];
        let json = build_classification_facts_with_plugins(facts, "n1", None, &plugin, now);
        assert_eq!(
            get_fact_value(&json, "acme.tier"),
            Some(serde_json::json!(1))
        );
        assert_eq!(
            get_fact_value(&json, "acme.owner"),
            Some(serde_json::json!("ops"))
        );
        // Older than max_stale_s: missing.
        assert_eq!(get_fact_value(&json, "acme.old"), None);
        // A namespace or name reported by the agent is never overwritten.
        assert_eq!(get_fact_value(&json, "os.custom"), None);
        assert_eq!(
            get_fact_value(&json, "os.family"),
            Some(serde_json::json!("RedHat"))
        );
        assert_eq!(
            get_fact_value(&json, "dup.owner"),
            Some(serde_json::json!("agent"))
        );
        // Without plugin facts the payload is what it always was.
        let plain = build_classification_facts(
            vec![agent("os.family", serde_json::json!("RedHat"))],
            "n1",
            None,
        );
        assert_eq!(
            plain,
            serde_json::json!({"os": {"family": "RedHat"}, "clientcert": "n1", "certname": "n1"})
        );
    }

    #[test]
    fn test_build_classification_facts_rebuilds_nested_paths() {
        let facts = vec![
            Fact {
                certname: "node1.example.com".to_string(),
                name: "os.family".to_string(),
                value: serde_json::json!("RedHat"),
                environment: Some("develop".to_string()),
            },
            Fact {
                certname: "node1.example.com".to_string(),
                name: "os.release.major".to_string(),
                value: serde_json::json!("9"),
                environment: Some("develop".to_string()),
            },
        ];

        let facts_json = build_classification_facts(facts, "node1.example.com", Some("develop"));

        assert_eq!(
            get_fact_value(&facts_json, "os.family"),
            Some(serde_json::json!("RedHat"))
        );
        assert_eq!(
            get_fact_value(&facts_json, "os.release.major"),
            Some(serde_json::json!("9"))
        );
        assert_eq!(
            get_fact_value(&facts_json, "catalog_environment"),
            Some(serde_json::json!("develop"))
        );
        assert_eq!(
            get_fact_value(&facts_json, "clientcert"),
            Some(serde_json::json!("node1.example.com"))
        );
    }

    #[test]
    fn test_match_value_equals() {
        assert!(match_value(
            &serde_json::json!("RedHat"),
            &RuleOperator::Equals,
            &serde_json::json!("RedHat")
        ));

        assert!(!match_value(
            &serde_json::json!("Debian"),
            &RuleOperator::Equals,
            &serde_json::json!("RedHat")
        ));
    }

    #[test]
    fn test_match_value_not_equals() {
        assert!(match_value(
            &serde_json::json!("Debian"),
            &RuleOperator::NotEquals,
            &serde_json::json!("RedHat")
        ));

        assert!(!match_value(
            &serde_json::json!("RedHat"),
            &RuleOperator::NotEquals,
            &serde_json::json!("RedHat")
        ));
    }

    #[test]
    fn test_match_value_regex() {
        assert!(match_value(
            &serde_json::json!("RedHat"),
            &RuleOperator::Regex,
            &serde_json::json!("^Red.*")
        ));

        assert!(!match_value(
            &serde_json::json!("Debian"),
            &RuleOperator::Regex,
            &serde_json::json!("^Red.*")
        ));
    }

    #[test]
    fn test_match_value_not_regex() {
        assert!(match_value(
            &serde_json::json!("Debian"),
            &RuleOperator::NotRegex,
            &serde_json::json!("^Red.*")
        ));

        assert!(!match_value(
            &serde_json::json!("RedHat"),
            &RuleOperator::NotRegex,
            &serde_json::json!("^Red.*")
        ));
    }

    #[test]
    fn test_match_value_in() {
        assert!(match_value(
            &serde_json::json!("production"),
            &RuleOperator::In,
            &serde_json::json!(["production", "staging"])
        ));

        assert!(!match_value(
            &serde_json::json!("development"),
            &RuleOperator::In,
            &serde_json::json!(["production", "staging"])
        ));
    }

    #[test]
    fn test_match_value_not_in() {
        assert!(match_value(
            &serde_json::json!("development"),
            &RuleOperator::NotIn,
            &serde_json::json!(["production", "staging"])
        ));

        assert!(!match_value(
            &serde_json::json!("production"),
            &RuleOperator::NotIn,
            &serde_json::json!(["production", "staging"])
        ));
    }

    #[test]
    fn test_match_value_ordering() {
        assert!(match_value(
            &serde_json::json!(10),
            &RuleOperator::GreaterThan,
            &serde_json::json!(5)
        ));
        assert!(match_value(
            &serde_json::json!(5),
            &RuleOperator::LessThan,
            &serde_json::json!(10)
        ));
        assert!(match_value(
            &serde_json::json!(5),
            &RuleOperator::LessThanOrEqual,
            &serde_json::json!(5)
        ));
        assert!(match_value(
            &serde_json::json!(10),
            &RuleOperator::GreaterThanOrEqual,
            &serde_json::json!(10)
        ));
    }

    #[test]
    fn test_classify_pinned_node() {
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "webservers".to_string(),
            pinned_nodes: vec!["web1.example.com".to_string()],
            classes: serde_json::json!({"profile::webserver": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);
        let facts = serde_json::json!({});

        let result = service.classify("web1.example.com", &facts);

        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].match_type, MatchType::Pinned);
        assert!(result
            .classes
            .as_object()
            .unwrap()
            .contains_key("profile::webserver"));
    }

    #[test]
    fn test_classify_by_rules() {
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "redhat_servers".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "os.family".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("RedHat"),
            }],
            classes: serde_json::json!({"profile::base": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);
        let facts = serde_json::json!({
            "os": {
                "family": "RedHat"
            }
        });

        let result = service.classify("node1.example.com", &facts);

        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].match_type, MatchType::Rules);
    }

    #[test]
    fn test_classify_inherits_child_groups() {
        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "parent".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "os.family".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("RedHat"),
            }],
            classes: serde_json::json!({"class_parent": {"p": "root"}}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "child".to_string(),
            parent_id: Some(parent_id),
            match_all_nodes: true, // Child inherits from parent when this is enabled
            classes: serde_json::json!({"class_child": {"child": true}, "class_parent": {"p": "child"}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);
        let facts = serde_json::json!({
            "os": {"family": "RedHat"}
        });

        let result = service.classify("node1.example.com", &facts);

        assert_eq!(result.groups.len(), 2);
        assert_eq!(result.groups[0].match_type, MatchType::Rules);
        assert_eq!(result.groups[1].match_type, MatchType::Rules); // Changed from Inherited since match_all_nodes uses Rules type
                                                                   // Classes should be merged with deep merge
        assert!(result
            .classes
            .as_object()
            .unwrap()
            .contains_key("class_parent"));
        assert!(result
            .classes
            .as_object()
            .unwrap()
            .contains_key("class_child"));
        // Deep merge: child's parameters override parent's for same class
        assert_eq!(
            result.classes["class_parent"]["p"],
            serde_json::json!("child")
        );
        assert_eq!(
            result.classes["class_child"]["child"],
            serde_json::json!(true)
        );
    }

    #[test]
    fn test_classify_pinned_inherits_children() {
        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "parent".to_string(),
            pinned_nodes: vec!["web1.example.com".to_string()],
            classes: serde_json::json!({"class_parent": {}}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "child".to_string(),
            parent_id: Some(parent_id),
            match_all_nodes: true, // Child inherits from parent when this is enabled
            classes: serde_json::json!({"class_child": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);
        let facts = serde_json::json!({});

        let result = service.classify("web1.example.com", &facts);

        assert_eq!(result.groups.len(), 2);
        assert_eq!(result.groups[0].match_type, MatchType::Pinned);
        assert_eq!(result.groups[1].match_type, MatchType::Rules); // Child has match_all_nodes=true, uses Rules type
        assert!(result
            .classes
            .as_object()
            .unwrap()
            .contains_key("class_parent"));
        assert!(result
            .classes
            .as_object()
            .unwrap()
            .contains_key("class_child"));
    }

    #[test]
    fn test_classify_rule_match_any() {
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "any_rule".to_string(),
            rule_match_type: RuleMatchType::Any,
            rules: vec![
                ClassificationRule {
                    id: Uuid::new_v4(),
                    fact_path: "os.family".to_string(),
                    operator: RuleOperator::Equals,
                    value: serde_json::json!("RedHat"),
                },
                ClassificationRule {
                    id: Uuid::new_v4(),
                    fact_path: "os.release.major".to_string(),
                    operator: RuleOperator::Equals,
                    value: serde_json::json!("9"),
                },
            ],
            classes: serde_json::json!({"class_any": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);
        let facts = serde_json::json!({
            "os": {
                "family": "RedHat",
                "release": {"major": "8"}
            }
        });

        let result = service.classify("node1.example.com", &facts);

        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].match_type, MatchType::Rules);
        assert_eq!(result.groups[0].matched_rules.len(), 1);
        assert!(result
            .classes
            .as_object()
            .unwrap()
            .contains_key("class_any"));
    }

    #[test]
    fn test_classify_environment_filter_matches() {
        // Group with specific environment requirement
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "production_servers".to_string(),
            environment: Some("production".to_string()),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "os.family".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("RedHat"),
            }],
            classes: serde_json::json!({"profile::prod": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        // Node in production environment should match
        let facts = serde_json::json!({
            "os": {"family": "RedHat"},
            "catalog_environment": "production"
        });

        let result = service.classify("node1.example.com", &facts);
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].name, "production_servers");
    }

    #[test]
    fn test_classify_environment_filter_no_match() {
        // Group with specific environment requirement
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "production_servers".to_string(),
            environment: Some("production".to_string()),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "os.family".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("RedHat"),
            }],
            classes: serde_json::json!({"profile::prod": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        // Node in different environment should NOT match
        let facts = serde_json::json!({
            "os": {"family": "RedHat"},
            "catalog_environment": "development"
        });

        let result = service.classify("node1.example.com", &facts);
        assert_eq!(result.groups.len(), 0);
    }

    #[test]
    fn test_classify_environment_wildcard_matches_all() {
        // Test that "*", "All", and "Any" match nodes from any environment
        let groups = vec![
            NodeGroup {
                id: Uuid::new_v4(),
                name: "wildcard_group".to_string(),
                environment: Some("*".to_string()),
                rules: vec![ClassificationRule {
                    id: Uuid::new_v4(),
                    fact_path: "os.family".to_string(),
                    operator: RuleOperator::Equals,
                    value: serde_json::json!("RedHat"),
                }],
                classes: serde_json::json!({"class1": {}}),
                ..Default::default()
            },
            NodeGroup {
                id: Uuid::new_v4(),
                name: "all_group".to_string(),
                environment: Some("All".to_string()),
                rules: vec![ClassificationRule {
                    id: Uuid::new_v4(),
                    fact_path: "os.family".to_string(),
                    operator: RuleOperator::Equals,
                    value: serde_json::json!("RedHat"),
                }],
                classes: serde_json::json!({"class2": {}}),
                ..Default::default()
            },
            NodeGroup {
                id: Uuid::new_v4(),
                name: "any_group".to_string(),
                environment: Some("any".to_string()),
                rules: vec![ClassificationRule {
                    id: Uuid::new_v4(),
                    fact_path: "os.family".to_string(),
                    operator: RuleOperator::Equals,
                    value: serde_json::json!("RedHat"),
                }],
                classes: serde_json::json!({"class3": {}}),
                ..Default::default()
            },
            NodeGroup {
                id: Uuid::new_v4(),
                name: "none_group".to_string(),
                environment: None,
                rules: vec![ClassificationRule {
                    id: Uuid::new_v4(),
                    fact_path: "os.family".to_string(),
                    operator: RuleOperator::Equals,
                    value: serde_json::json!("RedHat"),
                }],
                classes: serde_json::json!({"class4": {}}),
                ..Default::default()
            },
        ];

        let service = ClassificationService::new(groups);

        let facts = serde_json::json!({
            "os": {"family": "RedHat"},
            "catalog_environment": "production"
        });

        let result = service.classify("node1.example.com", &facts);

        // All 4 groups should match
        assert_eq!(result.groups.len(), 4);
        assert!(result.classes.as_object().unwrap().contains_key("class1"));
        assert!(result.classes.as_object().unwrap().contains_key("class2"));
        assert!(result.classes.as_object().unwrap().contains_key("class3"));
        assert!(result.classes.as_object().unwrap().contains_key("class4"));
    }

    #[test]
    fn test_classify_pinned_node_ignores_environment() {
        // Pinned nodes ALWAYS match regardless of environment
        // This allows environment assignment without bootstrap problems
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "production_servers".to_string(),
            environment: Some("production".to_string()),
            pinned_nodes: vec!["web1.example.com".to_string()],
            classes: serde_json::json!({"profile::prod": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        // Pinned node with different environment should STILL match
        let facts = serde_json::json!({
            "catalog_environment": "development"
        });

        let result = service.classify("web1.example.com", &facts);
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].match_type, MatchType::Pinned);
        // The group's environment should be returned
        assert_eq!(result.environment, Some("production".to_string()));

        // Pinned node in matching environment should also match
        let facts = serde_json::json!({
            "catalog_environment": "production"
        });

        let result = service.classify("web1.example.com", &facts);
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].match_type, MatchType::Pinned);
    }

    #[test]
    fn test_deep_hierarchy_inherits_all_ancestors() {
        // Test that a deeply nested group (grandchild) inherits from all ancestors
        // Hierarchy: Root -> Child -> Grandchild
        let root_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();
        let grandchild_id = Uuid::new_v4();

        let root = NodeGroup {
            id: root_id,
            name: "root".to_string(),
            environment: None, // No environment restriction for root
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "os.family".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("RedHat"),
            }],
            classes: serde_json::json!({
                "base": {"version": "1.0"},
                "common": {"root_param": "root_value"}
            }),
            variables: serde_json::json!({"root_var": "from_root"}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: child_id,
            name: "child".to_string(),
            parent_id: Some(root_id),
            environment: None, // No environment restriction (inherits from parent)
            match_all_nodes: true, // Required to inherit from parent
            classes: serde_json::json!({
                "middleware": {"version": "2.0"},
                "common": {"child_param": "child_value"} // Merges with root's common class
            }),
            variables: serde_json::json!({"child_var": "from_child"}),
            ..Default::default()
        };

        let grandchild = NodeGroup {
            id: grandchild_id,
            name: "grandchild".to_string(),
            parent_id: Some(child_id),
            environment: None,     // No environment (should inherit from child)
            match_all_nodes: true, // Required to inherit from parent
            classes: serde_json::json!({
                "application": {"version": "3.0"},
                "common": {"grandchild_param": "grandchild_value"} // Further merges common class
            }),
            variables: serde_json::json!({"grandchild_var": "from_grandchild"}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![root, child, grandchild]);
        let facts = serde_json::json!({
            "os": {"family": "RedHat"}
        });

        let result = service.classify("node1.example.com", &facts);

        // Should match all three groups
        assert_eq!(result.groups.len(), 3);
        assert_eq!(result.groups[0].name, "root");
        assert_eq!(result.groups[0].match_type, MatchType::Rules);
        assert_eq!(result.groups[1].name, "child");
        assert_eq!(result.groups[1].match_type, MatchType::Rules); // match_all_nodes=true uses Rules type
        assert_eq!(result.groups[2].name, "grandchild");
        assert_eq!(result.groups[2].match_type, MatchType::Rules); // match_all_nodes=true uses Rules type

        // Environment should be None since no group in the hierarchy has an environment
        assert_eq!(result.environment, None);

        // Classes should be merged from all three groups
        let classes = result.classes.as_object().unwrap();
        assert!(classes.contains_key("base")); // From root
        assert!(classes.contains_key("middleware")); // From child
        assert!(classes.contains_key("application")); // From grandchild
        assert!(classes.contains_key("common")); // Merged from all three

        // Check that common class has parameters from all levels (deep merge)
        let common = &classes["common"];
        assert_eq!(common["root_param"], serde_json::json!("root_value"));
        assert_eq!(common["child_param"], serde_json::json!("child_value"));
        assert_eq!(
            common["grandchild_param"],
            serde_json::json!("grandchild_value")
        );

        // Variables should be merged from all three groups
        let vars = result.variables.as_object().unwrap();
        assert_eq!(vars["root_var"], serde_json::json!("from_root"));
        assert_eq!(vars["child_var"], serde_json::json!("from_child"));
        assert_eq!(vars["grandchild_var"], serde_json::json!("from_grandchild"));
    }

    #[test]
    fn test_child_without_parent_match_pinned_still_matches() {
        // Test that a pinned child group DOES match even if its parent doesn't match via rules
        // This is the expected behavior: pinning is an explicit override that bypasses rules
        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "parent".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "os.family".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("RedHat"),
            }],
            classes: serde_json::json!({"parent_class": {}}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "child".to_string(),
            parent_id: Some(parent_id),
            pinned_nodes: vec!["node1.example.com".to_string()], // Pinned!
            classes: serde_json::json!({"child_class": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);
        let facts = serde_json::json!({
            "os": {"family": "Debian"} // Doesn't match parent's rule
        });

        let result = service.classify("node1.example.com", &facts);

        // Pinned child should match (Pinned), and the parent should appear as
        // Inherited because pinning to a leaf logically places the node in
        // every ancestor group too. Both parent and child classes/variables
        // are also merged into the result.
        assert_eq!(result.groups.len(), 2);
        let parent_match = result
            .groups
            .iter()
            .find(|g| g.name == "parent")
            .expect("parent should be inherited via pinned child");
        assert_eq!(parent_match.match_type, MatchType::Inherited);
        let child_match = result
            .groups
            .iter()
            .find(|g| g.name == "child")
            .expect("pinned child should match");
        assert_eq!(child_match.match_type, MatchType::Pinned);
        let classes = result.classes.as_object().unwrap();
        assert!(classes.contains_key("parent_class"));
        assert!(classes.contains_key("child_class"));
    }

    #[test]
    fn test_pinned_node_appears_in_every_ancestor_group() {
        // Regression: in production, every node was pinned to a leaf group,
        // and the pinning logic only added the leaf to matched_groups while
        // marking ancestors as "processed" — making ancestor groups (Linux,
        // Develop, etc.) always look empty in the UI even though they
        // logically contained every pinned node. The fix propagates pinned
        // membership up the entire ancestor chain.
        let linux_id = Uuid::new_v4();
        let linux = NodeGroup {
            id: linux_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };
        let develop_id = Uuid::new_v4();
        let develop = NodeGroup {
            id: develop_id,
            name: "Develop".to_string(),
            parent_id: Some(linux_id),
            is_environment_group: true,
            environment: Some("develop".to_string()),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "clientcert".to_string(),
                operator: RuleOperator::Regex,
                value: serde_json::json!(".*ds.*"),
            }],
            ..Default::default()
        };
        let leaf_id = Uuid::new_v4();
        let leaf = NodeGroup {
            id: leaf_id,
            name: "ADM-ESI".to_string(),
            parent_id: Some(develop_id),
            pinned_nodes: vec!["apldc1vds0044.fgv.br".to_string()],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![linux, develop, leaf]);
        // Empty facts on purpose — without the fix, Linux's `kernel = Linux`
        // would never match and the chain would never propagate. The pinning
        // path must work even with sparse facts.
        let facts = serde_json::json!({});

        let result = service.classify("apldc1vds0044.fgv.br", &facts);

        let names: Vec<&str> = result.groups.iter().map(|g| g.name.as_str()).collect();
        assert!(
            names.contains(&"Linux"),
            "Linux ancestor missing: {names:?}"
        );
        assert!(
            names.contains(&"Develop"),
            "Develop ancestor missing: {names:?}"
        );
        assert!(names.contains(&"ADM-ESI"), "Pinned leaf missing: {names:?}");

        let leaf_match = result.groups.iter().find(|g| g.name == "ADM-ESI").unwrap();
        assert_eq!(leaf_match.match_type, MatchType::Pinned);
        for ancestor in ["Linux", "Develop"] {
            let m = result.groups.iter().find(|g| g.name == ancestor).unwrap();
            assert_eq!(
                m.match_type,
                MatchType::Inherited,
                "ancestor {ancestor} should be Inherited"
            );
        }

        // Pinned-leaf inheritance also overrides environment with Develop's.
        assert_eq!(result.environment.as_deref(), Some("develop"));
    }

    #[test]
    fn test_unpinned_node_matches_ancestor_group_via_rules() {
        // Mirror image of the pinning regression: when a node is NOT pinned
        // anywhere but matches a parent group's rule (kernel=Linux) AND a
        // child group's rule (clientcert ~ .*ds.*), both groups must show
        // up via Rules. Catches future refactors that accidentally couple
        // ancestor evaluation to pinning state.
        let linux_id = Uuid::new_v4();
        let linux = NodeGroup {
            id: linux_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };
        let develop_id = Uuid::new_v4();
        let develop = NodeGroup {
            id: develop_id,
            name: "Develop".to_string(),
            parent_id: Some(linux_id),
            is_environment_group: true,
            environment: Some("develop".to_string()),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "clientcert".to_string(),
                operator: RuleOperator::Regex,
                value: serde_json::json!(".*ds.*"),
            }],
            ..Default::default()
        };
        // A leaf with no pinned nodes — make sure it doesn't accidentally
        // pull the unpinned node in.
        let leaf = NodeGroup {
            id: Uuid::new_v4(),
            name: "ADM-ESI".to_string(),
            parent_id: Some(develop_id),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![linux, develop, leaf]);
        // `clientcert` and `certname` are injected by build_classification_facts
        // in the API path; tests have to provide them explicitly.
        let facts = serde_json::json!({
            "kernel": "Linux",
            "clientcert": "webdc1vds0021.fgv.br",
            "certname": "webdc1vds0021.fgv.br",
        });

        let matched = service.classify("webdc1vds0021.fgv.br", &facts);
        let names: Vec<&str> = matched.groups.iter().map(|g| g.name.as_str()).collect();
        assert!(
            names.contains(&"Linux") && names.contains(&"Develop"),
            "expected Linux and Develop via Rules, got {names:?}"
        );
        assert!(
            !names.contains(&"ADM-ESI"),
            "ADM-ESI has no rules and node is not pinned; should not appear: {names:?}"
        );
        assert_eq!(
            matched
                .groups
                .iter()
                .find(|g| g.name == "Develop")
                .unwrap()
                .match_type,
            MatchType::Rules
        );
        assert_eq!(matched.environment.as_deref(), Some("develop"));

        // Negative: a node whose certname does not contain "ds" matches Linux
        // but not Develop.
        let other_facts = serde_json::json!({
            "kernel": "Linux",
            "clientcert": "notreallyaserver.fgv.br",
            "certname": "notreallyaserver.fgv.br",
        });
        let other = service.classify("notreallyaserver.fgv.br", &other_facts);
        let other_names: Vec<&str> = other.groups.iter().map(|g| g.name.as_str()).collect();
        assert!(
            other_names.contains(&"Linux"),
            "Linux should still match: {other_names:?}"
        );
        assert!(
            !other_names.contains(&"Develop"),
            "Develop's regex should not match this certname: {other_names:?}"
        );
    }

    #[test]
    fn test_node_pinned_in_one_chain_still_rule_matches_unrelated_chain() {
        // Cross-chain regression: a node pinned to one branch of the tree
        // must still pick up rule-based matches in unrelated branches.
        // Without proper isolation, the `processed_groups` short-circuit
        // could spill across chains and silence rule evaluation in groups
        // that aren't ancestors of the pinned group.
        let linux_id = Uuid::new_v4();
        let linux = NodeGroup {
            id: linux_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };
        let develop = NodeGroup {
            id: Uuid::new_v4(),
            name: "Develop".to_string(),
            parent_id: Some(linux_id),
            is_environment_group: true,
            environment: Some("develop".to_string()),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "clientcert".to_string(),
                operator: RuleOperator::Regex,
                value: serde_json::json!(".*ds.*"),
            }],
            ..Default::default()
        };
        // Independent root with no relation to Linux/Develop. The node is
        // pinned here.
        let standalone = NodeGroup {
            id: Uuid::new_v4(),
            name: "Compliance Audit".to_string(),
            pinned_nodes: vec!["mysdc1vds0001.fgv.br".to_string()],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![linux, develop, standalone]);
        let facts = serde_json::json!({
            "kernel": "Linux",
            "clientcert": "mysdc1vds0001.fgv.br",
            "certname": "mysdc1vds0001.fgv.br",
        });
        let result = service.classify("mysdc1vds0001.fgv.br", &facts);
        let names: Vec<&str> = result.groups.iter().map(|g| g.name.as_str()).collect();

        // All three should be present: pinned + both rule-matched ancestors.
        for expected in ["Compliance Audit", "Linux", "Develop"] {
            assert!(names.contains(&expected), "{expected} missing in {names:?}");
        }
        assert_eq!(
            result
                .groups
                .iter()
                .find(|g| g.name == "Compliance Audit")
                .unwrap()
                .match_type,
            MatchType::Pinned
        );
        assert_eq!(
            result
                .groups
                .iter()
                .find(|g| g.name == "Linux")
                .unwrap()
                .match_type,
            MatchType::Rules
        );
        assert_eq!(
            result
                .groups
                .iter()
                .find(|g| g.name == "Develop")
                .unwrap()
                .match_type,
            MatchType::Rules
        );
    }

    #[test]
    fn test_non_pinned_child_without_parent_match_should_not_match() {
        // Test that a non-pinned child group doesn't match if its parent doesn't match
        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "parent".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "os.family".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("RedHat"),
            }],
            classes: serde_json::json!({"parent_class": {}}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "child".to_string(),
            parent_id: Some(parent_id),
            match_all_nodes: true, // Would match all nodes, but parent must match first
            classes: serde_json::json!({"child_class": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);
        let facts = serde_json::json!({
            "os": {"family": "Debian"} // Doesn't match parent's rule
        });

        let result = service.classify("node1.example.com", &facts);

        // Neither parent nor child should match (child isn't pinned)
        assert_eq!(result.groups.len(), 0);
        assert!(result.classes.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_pinned_group_overrides_rule_matched_environment() {
        // Test that a pinned group's environment overrides a rule-matched group's environment
        // This simulates: node matches "Linux Servers" (production) AND is pinned to "Puppet Servers" (pserver)
        // Expected: pinned group's environment should take precedence

        let linux_servers = NodeGroup {
            id: Uuid::new_v4(),
            name: "Linux Servers".to_string(),
            environment: Some("production".to_string()),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            classes: serde_json::json!({"base": {}}),
            ..Default::default()
        };

        let puppet_servers = NodeGroup {
            id: Uuid::new_v4(),
            name: "Puppet Servers".to_string(),
            environment: Some("pserver".to_string()),
            pinned_nodes: vec!["segdc1vpr0018.fgv.br".to_string()],
            classes: serde_json::json!({"puppetserver": {}}),
            ..Default::default()
        };

        // Test scenario 1: Linux Servers processed first, then Puppet Servers
        let service1 =
            ClassificationService::new(vec![linux_servers.clone(), puppet_servers.clone()]);
        let facts = serde_json::json!({
            "kernel": "Linux",
            "catalog_environment": "pserver"  // Node is in pserver environment
        });

        let result1 = service1.classify("segdc1vpr0018.fgv.br", &facts);

        // Node should match Puppet Servers (pinned) but NOT Linux Servers (wrong environment)
        assert_eq!(result1.groups.len(), 1);
        assert_eq!(result1.groups[0].name, "Puppet Servers");
        assert_eq!(result1.groups[0].match_type, MatchType::Pinned);
        assert_eq!(result1.environment, Some("pserver".to_string()));
        assert!(result1
            .classes
            .as_object()
            .unwrap()
            .contains_key("puppetserver"));

        // Test scenario 2: Puppet Servers processed first, then Linux Servers
        let service2 =
            ClassificationService::new(vec![puppet_servers.clone(), linux_servers.clone()]);
        let result2 = service2.classify("segdc1vpr0018.fgv.br", &facts);

        // Same result regardless of order
        assert_eq!(result2.groups.len(), 1);
        assert_eq!(result2.groups[0].name, "Puppet Servers");
        assert_eq!(result2.environment, Some("pserver".to_string()));
    }

    #[test]
    fn test_environment_group_child_inherits_only_matching_nodes() {
        // Test scenario: Parent is an environment group with rules
        // Child has no rules, should only match nodes that match the parent
        //
        // Parent: "Homolog" (environment group) with rule clientcert ~ .*vhm.*
        // Child: "R - Docker Apps" with no rules
        //
        // Node A (segdc1vhm0001): certname contains "vhm" -> should match both
        // Node B (apldc1vds0045): certname doesn't contain "vhm" -> should match neither

        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "Homolog".to_string(),
            environment: Some("homolog".to_string()),
            is_environment_group: true, // This is an environment group
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "clientcert".to_string(),
                operator: RuleOperator::Regex,
                value: serde_json::json!(".*vhm.*"),
            }],
            classes: serde_json::json!({}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "R - Docker Apps".to_string(),
            parent_id: Some(parent_id),
            environment: None,     // No environment - inherits from parent
            match_all_nodes: true, // Required to inherit from parent
            // No rules - should inherit from parent
            rules: vec![],
            classes: serde_json::json!({"docker": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);

        // Node A: certname contains "vhm" - should match both parent and child
        let facts_a = serde_json::json!({
            "clientcert": "segdc1vhm0001.fgv.br"
        });
        let result_a = service.classify("segdc1vhm0001.fgv.br", &facts_a);
        assert_eq!(
            result_a.groups.len(),
            2,
            "Node with 'vhm' should match both groups"
        );
        assert_eq!(result_a.groups[0].name, "Homolog");
        assert_eq!(result_a.groups[1].name, "R - Docker Apps");
        assert_eq!(result_a.groups[1].match_type, MatchType::Rules); // match_all_nodes=true uses Rules type

        // Node B: certname doesn't contain "vhm" - should match neither
        let facts_b = serde_json::json!({
            "clientcert": "apldc1vds0045.fgv.br"
        });
        let result_b = service.classify("apldc1vds0045.fgv.br", &facts_b);
        assert_eq!(
            result_b.groups.len(),
            0,
            "Node without 'vhm' should not match any group"
        );
    }

    #[test]
    fn test_child_with_rules_must_also_match_parent() {
        // Test that even if a child group has its own rules that match,
        // the node must ALSO match the parent for the child to match

        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            classes: serde_json::json!({}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "Docker Hosts".to_string(),
            parent_id: Some(parent_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "docker_installed".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!(true),
            }],
            classes: serde_json::json!({"docker": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);

        // Node with Docker but NOT Linux - should not match child
        let facts = serde_json::json!({
            "kernel": "Windows",
            "docker_installed": true
        });
        let result = service.classify("win-docker.example.com", &facts);
        assert_eq!(
            result.groups.len(),
            0,
            "Windows node should not match even with Docker"
        );

        // Node with Linux AND Docker - should match both
        let facts = serde_json::json!({
            "kernel": "Linux",
            "docker_installed": true
        });
        let result = service.classify("linux-docker.example.com", &facts);
        assert_eq!(
            result.groups.len(),
            2,
            "Linux node with Docker should match both groups"
        );
    }

    #[test]
    fn test_match_all_nodes_root_group() {
        // Test that a root group with match_all_nodes=true matches all nodes
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "All Nodes".to_string(),
            match_all_nodes: true, // Matches all nodes
            rules: vec![],         // No rules
            classes: serde_json::json!({"base": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        // Any node should match
        let facts = serde_json::json!({
            "kernel": "Linux"
        });
        let result = service.classify("any-node.example.com", &facts);
        assert_eq!(
            result.groups.len(),
            1,
            "Node should match group with match_all_nodes=true"
        );
        assert_eq!(result.groups[0].name, "All Nodes");
    }

    #[test]
    fn test_match_all_nodes_false_root_group() {
        // Test that a root group with match_all_nodes=false and no rules matches NO nodes
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "Empty Group".to_string(),
            match_all_nodes: false, // Default - should not match when no rules
            rules: vec![],          // No rules
            classes: serde_json::json!({"base": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        let facts = serde_json::json!({
            "kernel": "Linux"
        });
        let result = service.classify("any-node.example.com", &facts);
        assert_eq!(
            result.groups.len(),
            0,
            "Node should NOT match group with match_all_nodes=false and no rules"
        );
    }

    #[test]
    fn test_match_all_nodes_child_respects_parent() {
        // Test that a child group with match_all_nodes=true still requires parent to match
        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            classes: serde_json::json!({}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "All Linux Nodes".to_string(),
            parent_id: Some(parent_id),
            match_all_nodes: true, // Matches all nodes within parent context
            rules: vec![],         // No rules
            classes: serde_json::json!({"linux_base": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);

        // Linux node should match both parent and child
        let linux_facts = serde_json::json!({
            "kernel": "Linux"
        });
        let result = service.classify("linux-node.example.com", &linux_facts);
        assert_eq!(
            result.groups.len(),
            2,
            "Linux node should match both groups"
        );
        assert_eq!(result.groups[0].name, "Linux");
        assert_eq!(result.groups[1].name, "All Linux Nodes");

        // Windows node should match neither (parent doesn't match)
        let windows_facts = serde_json::json!({
            "kernel": "Windows"
        });
        let result = service.classify("windows-node.example.com", &windows_facts);
        assert_eq!(
            result.groups.len(),
            0,
            "Windows node should not match - parent doesn't match"
        );
    }

    #[test]
    fn test_match_all_nodes_with_environment_filter() {
        // Test that match_all_nodes still respects environment filtering
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "Production All".to_string(),
            environment: Some("production".to_string()),
            is_environment_group: false, // Regular group - filters by environment
            match_all_nodes: true,       // Would match all nodes IF environment passes
            rules: vec![],
            classes: serde_json::json!({"production": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        // Node in production environment should match
        let prod_facts = serde_json::json!({
            "catalog_environment": "production"
        });
        let result = service.classify("prod-node.example.com", &prod_facts);
        assert_eq!(result.groups.len(), 1, "Production node should match");

        // Node in staging environment should NOT match
        let staging_facts = serde_json::json!({
            "catalog_environment": "staging"
        });
        let result = service.classify("staging-node.example.com", &staging_facts);
        assert_eq!(
            result.groups.len(),
            0,
            "Staging node should not match - wrong environment"
        );
    }

    #[test]
    fn test_child_with_rules_does_not_match_when_parent_has_no_rules_and_no_match_all() {
        // Test scenario from user bug report:
        // - Grandparent: "Homolog" (has rules, matches)
        // - Parent: "ADM - ESI (H)" (0 rules, match_all_nodes=false) -> should NOT match
        // - Child: "R - Docker Apps" (has rules that match) -> should NOT match because parent doesn't
        //
        // Even if a child group has rules that match the node,
        // it should NOT match if its parent doesn't match.

        let grandparent_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();

        let grandparent = NodeGroup {
            id: grandparent_id,
            name: "Homolog".to_string(),
            environment: Some("homolog".to_string()),
            is_environment_group: true,
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "clientcert".to_string(),
                operator: RuleOperator::Regex,
                value: serde_json::json!(".*vhm.*"),
            }],
            classes: serde_json::json!({}),
            ..Default::default()
        };

        let parent = NodeGroup {
            id: parent_id,
            name: "ADM - ESI (H)".to_string(),
            parent_id: Some(grandparent_id),
            match_all_nodes: false, // No rules + match_all_nodes=false = matches NO nodes
            rules: vec![],          // No rules!
            classes: serde_json::json!({}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "R - Docker Apps".to_string(),
            parent_id: Some(parent_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "clientcert".to_string(),
                operator: RuleOperator::Regex,
                value: serde_json::json!(".*vhm.*"),
            }],
            classes: serde_json::json!({"docker": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![grandparent, parent, child]);

        // Node that matches grandparent and child rules, but parent has no rules
        let facts = serde_json::json!({
            "clientcert": "segdc1vhm0001.fgv.br"
        });
        let result = service.classify("segdc1vhm0001.fgv.br", &facts);

        // Should only match grandparent (Homolog), NOT parent or child
        // Because parent has no rules and match_all_nodes=false, it doesn't match
        // And since parent doesn't match, child shouldn't even be evaluated
        assert_eq!(
            result.groups.len(),
            1,
            "Should only match grandparent, not parent or child. Got: {:?}",
            result.groups.iter().map(|g| &g.name).collect::<Vec<_>>()
        );
        assert_eq!(result.groups[0].name, "Homolog");
    }

    // =========================================================================
    // Comprehensive Classification Scenario Tests
    // =========================================================================

    #[test]
    fn test_scenario_simple_root_group_with_rules() {
        // Scenario: Single root group with rules
        // - Group: "Linux Servers" with rule kernel=Linux
        // - Node A: kernel=Linux -> should match
        // - Node B: kernel=Windows -> should not match

        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "Linux Servers".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        // Node A: Linux
        let result = service.classify("linux.example.com", &serde_json::json!({"kernel": "Linux"}));
        assert_eq!(result.groups.len(), 1, "Linux node should match");
        assert_eq!(result.groups[0].name, "Linux Servers");

        // Node B: Windows
        let result = service.classify(
            "windows.example.com",
            &serde_json::json!({"kernel": "Windows"}),
        );
        assert_eq!(result.groups.len(), 0, "Windows node should not match");
    }

    #[test]
    fn test_scenario_root_group_no_rules_match_all_false() {
        // Scenario: Root group with no rules and match_all_nodes=false
        // - Group: "Empty Group" with no rules, match_all_nodes=false
        // - Any node -> should NOT match

        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "Empty Group".to_string(),
            match_all_nodes: false,
            rules: vec![],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        let result = service.classify("any.example.com", &serde_json::json!({"kernel": "Linux"}));
        assert_eq!(
            result.groups.len(),
            0,
            "No node should match group with no rules and match_all_nodes=false"
        );
    }

    #[test]
    fn test_scenario_root_group_no_rules_match_all_true() {
        // Scenario: Root group with no rules and match_all_nodes=true
        // - Group: "All Nodes" with no rules, match_all_nodes=true
        // - Any node -> should match

        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "All Nodes".to_string(),
            match_all_nodes: true,
            rules: vec![],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        let result = service.classify("any.example.com", &serde_json::json!({"kernel": "Linux"}));
        assert_eq!(
            result.groups.len(),
            1,
            "All nodes should match group with match_all_nodes=true"
        );
        assert_eq!(result.groups[0].name, "All Nodes");
    }

    #[test]
    fn test_scenario_parent_with_rules_child_with_rules() {
        // Scenario: Parent has rules, child has rules
        // - Parent: "Linux" with rule kernel=Linux
        // - Child: "Web Servers" with rule role=webserver
        // - Node A: kernel=Linux, role=webserver -> matches both
        // - Node B: kernel=Linux, role=dbserver -> matches parent only
        // - Node C: kernel=Windows, role=webserver -> matches neither

        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "Web Servers".to_string(),
            parent_id: Some(parent_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "role".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("webserver"),
            }],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);

        // Node A: Linux + webserver -> both
        let result = service.classify(
            "web.example.com",
            &serde_json::json!({"kernel": "Linux", "role": "webserver"}),
        );
        assert_eq!(result.groups.len(), 2, "Node A should match both groups");

        // Node B: Linux + dbserver -> parent only
        let result = service.classify(
            "db.example.com",
            &serde_json::json!({"kernel": "Linux", "role": "dbserver"}),
        );
        assert_eq!(result.groups.len(), 1, "Node B should match parent only");
        assert_eq!(result.groups[0].name, "Linux");

        // Node C: Windows + webserver -> neither
        let result = service.classify(
            "winweb.example.com",
            &serde_json::json!({"kernel": "Windows", "role": "webserver"}),
        );
        assert_eq!(result.groups.len(), 0, "Node C should match neither");
    }

    #[test]
    fn test_scenario_parent_with_rules_child_no_rules_match_all_false() {
        // Scenario: Parent has rules, child has NO rules, match_all_nodes=false
        // - Parent: "Linux" with rule kernel=Linux
        // - Child: "Empty Child" with no rules, match_all_nodes=false
        // - Node A: kernel=Linux -> matches parent only (child has no rules)
        // - Node B: kernel=Windows -> matches neither

        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "Empty Child".to_string(),
            parent_id: Some(parent_id),
            match_all_nodes: false, // Child has no rules and match_all_nodes=false
            rules: vec![],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);

        // Node A: Linux -> parent only
        let result = service.classify("linux.example.com", &serde_json::json!({"kernel": "Linux"}));
        assert_eq!(
            result.groups.len(),
            1,
            "Node A should match parent only, not empty child"
        );
        assert_eq!(result.groups[0].name, "Linux");

        // Node B: Windows -> neither
        let result = service.classify(
            "windows.example.com",
            &serde_json::json!({"kernel": "Windows"}),
        );
        assert_eq!(result.groups.len(), 0, "Node B should match neither");
    }

    #[test]
    fn test_scenario_parent_with_rules_child_no_rules_match_all_true() {
        // Scenario: Parent has rules, child has NO rules, match_all_nodes=true
        // - Parent: "Linux" with rule kernel=Linux
        // - Child: "All Linux" with no rules, match_all_nodes=true
        // - Node A: kernel=Linux -> matches both (child inherits from parent)
        // - Node B: kernel=Windows -> matches neither (parent doesn't match, so child isn't evaluated)

        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "All Linux".to_string(),
            parent_id: Some(parent_id),
            match_all_nodes: true, // Child has no rules but match_all_nodes=true
            rules: vec![],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);

        // Node A: Linux -> both
        let result = service.classify("linux.example.com", &serde_json::json!({"kernel": "Linux"}));
        assert_eq!(result.groups.len(), 2, "Node A should match both groups");
        assert_eq!(result.groups[0].name, "Linux");
        assert_eq!(result.groups[1].name, "All Linux");

        // Node B: Windows -> neither
        let result = service.classify(
            "windows.example.com",
            &serde_json::json!({"kernel": "Windows"}),
        );
        assert_eq!(
            result.groups.len(),
            0,
            "Node B should match neither (parent didn't match)"
        );
    }

    #[test]
    fn test_scenario_parent_no_rules_match_all_false_child_with_rules() {
        // Scenario: Parent has NO rules (match_all_nodes=false), child HAS rules
        // This is the bug scenario reported by the user!
        // - Parent: "Empty Parent" with no rules, match_all_nodes=false
        // - Child: "Specific Child" with rule role=webserver
        // - Node A: role=webserver -> should match NEITHER (parent doesn't match, child not evaluated)

        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "Empty Parent".to_string(),
            match_all_nodes: false,
            rules: vec![],
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "Specific Child".to_string(),
            parent_id: Some(parent_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "role".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("webserver"),
            }],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);

        // Node A: webserver -> NEITHER (parent doesn't match)
        let result = service.classify("web.example.com", &serde_json::json!({"role": "webserver"}));
        assert_eq!(
            result.groups.len(),
            0,
            "Node should not match child because parent has no rules and match_all_nodes=false"
        );
    }

    #[test]
    fn test_scenario_three_level_hierarchy_all_with_rules() {
        // Scenario: Three-level hierarchy, all groups have rules
        // - Root: "Linux" with rule kernel=Linux
        // - Child: "Web" with rule role=webserver
        // - Grandchild: "Nginx" with rule app=nginx
        // Test various combinations

        let root_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        let root = NodeGroup {
            id: root_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };

        let child = NodeGroup {
            id: child_id,
            name: "Web".to_string(),
            parent_id: Some(root_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "role".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("webserver"),
            }],
            ..Default::default()
        };

        let grandchild = NodeGroup {
            id: Uuid::new_v4(),
            name: "Nginx".to_string(),
            parent_id: Some(child_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "app".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("nginx"),
            }],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![root, child, grandchild]);

        // Node: Linux + webserver + nginx -> all three
        let result = service.classify(
            "nginx.example.com",
            &serde_json::json!({"kernel": "Linux", "role": "webserver", "app": "nginx"}),
        );
        assert_eq!(result.groups.len(), 3, "Should match all three");

        // Node: Linux + webserver + apache -> root and child only
        let result = service.classify(
            "apache.example.com",
            &serde_json::json!({"kernel": "Linux", "role": "webserver", "app": "apache"}),
        );
        assert_eq!(result.groups.len(), 2, "Should match root and child only");

        // Node: Linux + dbserver + nginx -> root only
        let result = service.classify(
            "db.example.com",
            &serde_json::json!({"kernel": "Linux", "role": "dbserver", "app": "nginx"}),
        );
        assert_eq!(result.groups.len(), 1, "Should match root only");
        assert_eq!(result.groups[0].name, "Linux");

        // Node: Windows + webserver + nginx -> none
        let result = service.classify(
            "winweb.example.com",
            &serde_json::json!({"kernel": "Windows", "role": "webserver", "app": "nginx"}),
        );
        assert_eq!(result.groups.len(), 0, "Should match none");
    }

    #[test]
    fn test_scenario_three_level_middle_no_rules_match_all_false() {
        // Scenario: Three-level hierarchy, middle group has no rules with match_all_nodes=false
        // - Root: "Linux" with rule kernel=Linux
        // - Child: "Empty Middle" with NO rules, match_all_nodes=false
        // - Grandchild: "Specific" with rule app=nginx
        //
        // Since middle has no rules and match_all_nodes=false, it won't match,
        // so grandchild won't be evaluated!

        let root_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        let root = NodeGroup {
            id: root_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };

        let child = NodeGroup {
            id: child_id,
            name: "Empty Middle".to_string(),
            parent_id: Some(root_id),
            match_all_nodes: false, // No rules, match_all_nodes=false
            rules: vec![],
            ..Default::default()
        };

        let grandchild = NodeGroup {
            id: Uuid::new_v4(),
            name: "Specific".to_string(),
            parent_id: Some(child_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "app".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("nginx"),
            }],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![root, child, grandchild]);

        // Node: Linux + nginx -> should only match root!
        // Middle doesn't match (no rules, match_all_nodes=false), so grandchild isn't evaluated
        let result = service.classify(
            "nginx.example.com",
            &serde_json::json!({"kernel": "Linux", "app": "nginx"}),
        );
        assert_eq!(
            result.groups.len(),
            1,
            "Should match root only, middle blocks grandchild. Got: {:?}",
            result.groups.iter().map(|g| &g.name).collect::<Vec<_>>()
        );
        assert_eq!(result.groups[0].name, "Linux");
    }

    #[test]
    fn test_scenario_three_level_middle_no_rules_match_all_true() {
        // Scenario: Three-level hierarchy, middle group has no rules with match_all_nodes=true
        // - Root: "Linux" with rule kernel=Linux
        // - Child: "All Linux" with NO rules, match_all_nodes=true
        // - Grandchild: "Specific" with rule app=nginx
        //
        // Middle has match_all_nodes=true, so it matches all Linux nodes
        // Grandchild is then evaluated and matches if app=nginx

        let root_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        let root = NodeGroup {
            id: root_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };

        let child = NodeGroup {
            id: child_id,
            name: "All Linux".to_string(),
            parent_id: Some(root_id),
            match_all_nodes: true, // No rules, but match_all_nodes=true
            rules: vec![],
            ..Default::default()
        };

        let grandchild = NodeGroup {
            id: Uuid::new_v4(),
            name: "Nginx".to_string(),
            parent_id: Some(child_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "app".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("nginx"),
            }],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![root, child, grandchild]);

        // Node: Linux + nginx -> all three
        let result = service.classify(
            "nginx.example.com",
            &serde_json::json!({"kernel": "Linux", "app": "nginx"}),
        );
        assert_eq!(result.groups.len(), 3, "Should match all three");

        // Node: Linux + apache -> root and middle only
        let result = service.classify(
            "apache.example.com",
            &serde_json::json!({"kernel": "Linux", "app": "apache"}),
        );
        assert_eq!(result.groups.len(), 2, "Should match root and middle");
        assert_eq!(result.groups[0].name, "Linux");
        assert_eq!(result.groups[1].name, "All Linux");
    }

    #[test]
    fn test_scenario_sibling_groups_independent() {
        // Scenario: Sibling groups should be evaluated independently
        // - Root: "Linux" with rule kernel=Linux
        // - Child A: "Web" with rule role=webserver
        // - Child B: "DB" with rule role=dbserver
        //
        // A node can match root + one child, or root + both children if it matches both

        let root_id = Uuid::new_v4();

        let root = NodeGroup {
            id: root_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };

        let child_a = NodeGroup {
            id: Uuid::new_v4(),
            name: "Web".to_string(),
            parent_id: Some(root_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "role".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("webserver"),
            }],
            ..Default::default()
        };

        let child_b = NodeGroup {
            id: Uuid::new_v4(),
            name: "DB".to_string(),
            parent_id: Some(root_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "role".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("dbserver"),
            }],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![root, child_a, child_b]);

        // Node: Linux + webserver -> root + Web
        let result = service.classify(
            "web.example.com",
            &serde_json::json!({"kernel": "Linux", "role": "webserver"}),
        );
        assert_eq!(result.groups.len(), 2);

        // Node: Linux + dbserver -> root + DB
        let result = service.classify(
            "db.example.com",
            &serde_json::json!({"kernel": "Linux", "role": "dbserver"}),
        );
        assert_eq!(result.groups.len(), 2);

        // Node: Linux only -> root only
        let result = service.classify(
            "linux.example.com",
            &serde_json::json!({"kernel": "Linux", "role": "other"}),
        );
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].name, "Linux");
    }

    #[test]
    fn test_scenario_pinned_node_bypasses_rules() {
        // Scenario: Pinned nodes should match regardless of rules
        // - Group: "Special" with rule role=special, pinned_nodes=["pinned.example.com"]
        // - Node A (pinned): should match even without matching facts
        // - Node B (not pinned, matches rule): should match
        // - Node C (not pinned, doesn't match rule): should not match

        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "Special".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "role".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("special"),
            }],
            pinned_nodes: vec!["pinned.example.com".to_string()],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        // Node A: pinned -> matches even with wrong facts
        let result = service.classify("pinned.example.com", &serde_json::json!({"role": "other"}));
        assert_eq!(result.groups.len(), 1, "Pinned node should match");
        assert_eq!(result.groups[0].match_type, MatchType::Pinned);

        // Node B: not pinned but matches rule
        let result = service.classify(
            "special.example.com",
            &serde_json::json!({"role": "special"}),
        );
        assert_eq!(result.groups.len(), 1, "Rule-matching node should match");
        assert_eq!(result.groups[0].match_type, MatchType::Rules);

        // Node C: not pinned, doesn't match rule
        let result = service.classify("other.example.com", &serde_json::json!({"role": "other"}));
        assert_eq!(result.groups.len(), 0, "Non-matching node should not match");
    }

    // ---- M26: expressions, behavior policies, explain ----

    mod advanced {
        use super::*;
        use crate::models::{GroupBehavior, Inheritance, MatchMode, MergeStrategy};
        use chrono::TimeZone;
        use serde_json::json;

        fn now() -> DateTime<Utc> {
            Utc.with_ymd_and_hms(2026, 10, 28, 3, 15, 42).unwrap()
        }

        fn svc(groups: Vec<NodeGroup>) -> ClassificationService {
            ClassificationService::new(groups).with_now(now())
        }

        fn facts() -> serde_json::Value {
            json!({"os": {"family": "RedHat"}, "virtual": "kvm", "certname": "n1",
                   "catalog_environment": "production"})
        }

        fn rule(path: &str, value: serde_json::Value) -> ClassificationRule {
            ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: path.into(),
                operator: RuleOperator::Equals,
                value,
            }
        }

        fn expr_group(name: &str, expression: serde_json::Value) -> NodeGroup {
            NodeGroup {
                name: name.into(),
                expression: Some(expression),
                classes: json!({name: {}}),
                ..Default::default()
            }
        }

        fn names(r: &ClassificationResult) -> Vec<String> {
            r.groups.iter().map(|g| g.name.clone()).collect()
        }

        #[test]
        fn expression_alone_and_together_with_rules() {
            let e = json!({"any": [{"fact": "os.family", "op": "=", "value": "RedHat"},
                                   {"fact": "os.family", "op": "=", "value": "Debian"}]});
            let r = svc(vec![expr_group("e", e.clone())]).classify("n1", &facts());
            assert_eq!(names(&r), ["e"]);

            let mut both = expr_group("both", e);
            both.rules = vec![rule("virtual", json!("physical"))];
            let r = svc(vec![both.clone()]).classify("n1", &facts());
            assert!(r.groups.is_empty(), "rules and expression apply in AND");
            both.rules = vec![rule("virtual", json!("kvm"))];
            assert_eq!(names(&svc(vec![both]).classify("n1", &facts())), ["both"]);
        }

        #[test]
        fn invalid_stored_expression_never_matches() {
            let g = expr_group("bad", json!({"fn": "nope", "args": []}));
            let (r, ex) = svc(vec![g]).classify_explained("n1", &facts());
            assert!(r.groups.is_empty());
            assert!(ex.groups[0].reason.contains("invalid"));
        }

        #[test]
        fn higher_priority_sibling_wins_whatever_the_order() {
            let mk = |name: &str, prio: i32, v: &str| NodeGroup {
                name: name.into(),
                priority: prio,
                match_all_nodes: true,
                variables: json!({"tier": v}),
                ..Default::default()
            };
            for order in [[0, 1], [1, 0]] {
                let all = [mk("low", 50, "low"), mk("high", 200, "high")];
                let groups = order.iter().map(|i| all[*i].clone()).collect();
                let r = svc(groups).classify("n1", &facts());
                assert_eq!(r.variables["tier"], "high");
            }
        }

        #[test]
        fn exclusive_set_keeps_only_the_highest_priority_match() {
            let mk = |name: &str, prio: i32| NodeGroup {
                name: name.into(),
                priority: prio,
                match_all_nodes: true,
                classes: json!({name: {}}),
                behavior: GroupBehavior {
                    exclusive_set: Some("tier".into()),
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut other = mk("not_in_set", 1);
            other.behavior.exclusive_set = None;
            let (r, ex) = svc(vec![mk("gold", 300), mk("silver", 200), other])
                .classify_explained("n1", &facts());
            assert_eq!(
                names(&r),
                ["not_in_set", "gold"],
                "lower priority is walked first"
            );
            let silver = ex.groups.iter().find(|g| g.name == "silver").unwrap();
            assert_eq!(silver.outcome, GroupOutcome::Excluded);
            assert!(silver.reason.contains("gold"));
            assert!(r.classes.get("silver").is_none());
        }

        #[test]
        fn match_mode_n_of_counts_rules() {
            let mut g = NodeGroup {
                name: "g".into(),
                rules: vec![
                    rule("os.family", json!("RedHat")),
                    rule("virtual", json!("physical")),
                    rule("certname", json!("n1")),
                ],
                ..Default::default()
            };
            assert!(svc(vec![g.clone()])
                .classify("n1", &facts())
                .groups
                .is_empty());
            g.behavior.match_mode = Some(MatchMode::NOf { k: 2 });
            assert_eq!(
                svc(vec![g.clone()]).classify("n1", &facts()).groups.len(),
                1
            );
            g.behavior.match_mode = Some(MatchMode::NOf { k: 3 });
            assert!(svc(vec![g]).classify("n1", &facts()).groups.is_empty());
        }

        fn tree(child_behavior: GroupBehavior, parent_matches: bool) -> Vec<NodeGroup> {
            let parent = NodeGroup {
                name: "parent".into(),
                match_all_nodes: parent_matches,
                classes: json!({"base": {"a": 1}, "ntp": {}}),
                variables: json!({"owner": "ops", "site": "x"}),
                ..Default::default()
            };
            let child = NodeGroup {
                name: "child".into(),
                parent_id: Some(parent.id),
                match_all_nodes: true,
                classes: json!({"web": {}}),
                variables: json!({"role": "web"}),
                behavior: child_behavior,
                ..Default::default()
            };
            vec![parent, child]
        }

        #[test]
        fn inheritance_block_and_select_cut_ancestor_values() {
            let r = svc(tree(GroupBehavior::default(), true)).classify("n1", &facts());
            assert!(r.classes.get("base").is_some() && r.variables.get("owner").is_some());

            let block = GroupBehavior {
                inheritance: Inheritance::Block,
                ..Default::default()
            };
            let r = svc(tree(block, true)).classify("n1", &facts());
            assert_eq!(r.classes, json!({"web": {}}));
            assert_eq!(r.variables, json!({"role": "web"}));
            assert_eq!(names(&r), ["parent", "child"], "membership is unchanged");

            let select = GroupBehavior {
                inheritance: Inheritance::Select {
                    keys: vec!["ntp".into(), "owner".into()],
                },
                ..Default::default()
            };
            let r = svc(tree(select, true)).classify("n1", &facts());
            assert_eq!(r.classes, json!({"ntp": {}, "web": {}}));
            assert_eq!(r.variables, json!({"owner": "ops", "role": "web"}));
        }

        #[test]
        fn independent_group_matches_without_its_parent_and_inherits_nothing_unmatched() {
            let independent = GroupBehavior {
                inheritance: Inheritance::Independent,
                ..Default::default()
            };
            let r = svc(tree(independent, false)).classify("n1", &facts());
            assert_eq!(names(&r), ["child"]);
            assert_eq!(r.classes, json!({"web": {}}));

            let r = svc(tree(GroupBehavior::default(), false)).classify("n1", &facts());
            assert!(r.groups.is_empty());

            let independent = GroupBehavior {
                inheritance: Inheritance::Independent,
                ..Default::default()
            };
            let r = svc(tree(independent, true)).classify("n1", &facts());
            assert_eq!(names(&r), ["parent", "child"]);
            assert!(
                r.classes.get("base").is_some(),
                "a matched parent still contributes"
            );
        }

        #[test]
        fn merge_strategies() {
            let base = NodeGroup {
                name: "base".into(),
                match_all_nodes: true,
                classes: json!({"ntp": {"servers": ["a"], "port": 1}}),
                variables: json!({"list": ["a"], "k": 1}),
                ..Default::default()
            };
            let child = |merge| NodeGroup {
                name: "child".into(),
                parent_id: Some(base.id),
                match_all_nodes: true,
                classes: json!({"ntp": {"servers": ["b"]}}),
                variables: json!({"list": ["b"], "k": 2}),
                behavior: GroupBehavior {
                    merge,
                    ..Default::default()
                },
                ..Default::default()
            };
            let run = |m| svc(vec![base.clone(), child(m)]).classify("n1", &facts());

            let r = run(MergeStrategy::Deep);
            assert_eq!(r.classes["ntp"], json!({"servers": ["b"], "port": 1}));
            assert_eq!(r.variables["list"], json!(["b"]));

            let r = run(MergeStrategy::Replace);
            assert_eq!(r.classes["ntp"], json!({"servers": ["b"]}));

            let r = run(MergeStrategy::Union);
            assert_eq!(r.classes["ntp"]["servers"], json!(["a", "b"]));
            assert_eq!(r.variables["list"], json!(["a", "b"]));
            assert_eq!(r.variables["k"], 2);

            let r = run(MergeStrategy::FailOnConflict);
            assert_eq!(r.variables["k"], 1, "the first value is kept");
            let err = r.conflict_error.expect("conflict reported");
            assert!(
                err.contains("variable k") && err.contains("class ntp"),
                "{err}"
            );
        }

        #[test]
        fn active_window_gates_membership() {
            let mk = |from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>| NodeGroup {
                name: "w".into(),
                match_all_nodes: true,
                behavior: GroupBehavior {
                    active_from: from,
                    expires_at: to,
                    ..Default::default()
                },
                ..Default::default()
            };
            let hour = |h| Utc.with_ymd_and_hms(2026, 10, 28, h, 0, 0).unwrap();
            let run = |g: NodeGroup| svc(vec![g]).classify("n1", &facts()).groups.len();
            assert_eq!(run(mk(Some(hour(3)), Some(hour(4)))), 1);
            assert_eq!(run(mk(Some(hour(4)), None)), 0, "not active yet");
            assert_eq!(run(mk(None, Some(hour(3)))), 0, "expired");
            let mut pinned = mk(None, Some(hour(3)));
            pinned.pinned_nodes = vec!["n1".into()];
            assert_eq!(run(pinned), 0, "a window also applies to pinned nodes");
        }

        #[test]
        fn explain_reports_reasons_values_and_provenance() {
            let root = NodeGroup {
                name: "root".into(),
                environment: Some("production".into()),
                is_environment_group: true,
                rules: vec![rule("os.family", json!("RedHat"))],
                classes: json!({"base": {}}),
                variables: json!({"owner": "ops"}),
                ..Default::default()
            };
            let child = NodeGroup {
                name: "child".into(),
                parent_id: Some(root.id),
                expression: Some(json!({"not": {"fact": "virtual", "op": "=", "value": "kvm"}})),
                classes: json!({"web": {}}),
                variables: json!({"owner": "web"}),
                ..Default::default()
            };
            let hidden = NodeGroup {
                name: "hidden".into(),
                parent_id: Some(child.id),
                match_all_nodes: true,
                ..Default::default()
            };
            let (r, ex) =
                svc(vec![root.clone(), child.clone(), hidden]).classify_explained("n1", &facts());
            assert_eq!(names(&r), ["root"]);
            let by = |n: &str| ex.groups.iter().find(|g| g.name == n).unwrap();
            assert_eq!(by("root").outcome, GroupOutcome::Matched);
            assert_eq!(by("root").rules[0].actual, Some(json!("RedHat")));
            assert_eq!(by("child").outcome, GroupOutcome::NotMatched);
            let trace = by("child").expression.as_ref().unwrap();
            assert_eq!(trace.kind, "not");
            assert_eq!(trace.children[0].left, Some(json!("kvm")));
            assert_eq!(by("hidden").outcome, GroupOutcome::NotEvaluated);
            assert!(by("hidden").reason.contains("child"));
            assert_eq!(
                ex.provenance.environment.as_ref().unwrap().group_name,
                "root"
            );

            // With the child matching, the last origin is the winner.
            let mut f = facts();
            f["virtual"] = json!("physical");
            let (r, ex) = svc(vec![root, child]).classify_explained("n1", &f);
            assert_eq!(r.variables["owner"], "web");
            let owners = &ex.provenance.variables["owner"];
            assert_eq!(
                owners
                    .iter()
                    .map(|o| o.group_name.as_str())
                    .collect::<Vec<_>>(),
                ["root", "child"]
            );
            assert_eq!(ex.provenance.classes["web"].len(), 1);
        }

        #[test]
        fn explain_does_not_change_the_result() {
            let groups = tree(GroupBehavior::default(), true);
            let s = svc(groups);
            let plain = s.classify("n1", &facts());
            let (explained, _) = s.classify_explained("n1", &facts());
            assert_eq!(
                serde_json::to_value(&plain).unwrap(),
                serde_json::to_value(&explained).unwrap()
            );
        }
    }
}
