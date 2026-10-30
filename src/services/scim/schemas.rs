//! Discovery documents (RFC 7643 §5-§7, RFC 7644 §4): ServiceProviderConfig,
//! ResourceTypes and Schemas. They describe what the host stores.

use serde_json::{json, Value};

use super::{
    ScimConfig, GROUP_SCHEMA, RESOURCE_TYPE_SCHEMA, SCHEMA_SCHEMA, SPC_SCHEMA, USER_SCHEMA,
};

pub fn service_provider_config(cfg: &ScimConfig, base: &str) -> Value {
    json!({
        "schemas": [SPC_SCHEMA],
        "patch": {"supported": true},
        "bulk": {"supported": false, "maxOperations": 0, "maxPayloadSize": 0},
        "filter": {"supported": true, "maxResults": cfg.max_page_size},
        "changePassword": {"supported": false},
        "sort": {"supported": false},
        "etag": {"supported": true},
        "authenticationSchemes": [{
            "type": "oauthbearertoken",
            "name": "Bearer token",
            "description": "A SCIM token issued by an OpenVox WebUI administrator, sent as 'Authorization: Bearer <token>'",
            "primary": true
        }],
        "meta": {
            "resourceType": "ServiceProviderConfig",
            "location": format!("{base}/ServiceProviderConfig")
        }
    })
}

fn resource_type(id: &str, endpoint: &str, schema: &str, base: &str) -> Value {
    json!({
        "schemas": [RESOURCE_TYPE_SCHEMA],
        "id": id,
        "name": id,
        "endpoint": endpoint,
        "description": format!("{id} provisioned by SCIM"),
        "schema": schema,
        "schemaExtensions": [],
        "meta": {"resourceType": "ResourceType", "location": format!("{base}/ResourceTypes/{id}")}
    })
}

pub fn resource_types(base: &str) -> Vec<Value> {
    vec![
        resource_type("User", "/Users", USER_SCHEMA, base),
        resource_type("Group", "/Groups", GROUP_SCHEMA, base),
    ]
}

struct A {
    name: &'static str,
    kind: &'static str,
    multi: bool,
    required: bool,
    case_exact: bool,
    mutability: &'static str,
    uniqueness: &'static str,
    sub: Vec<A>,
}

fn a(name: &'static str, kind: &'static str) -> A {
    A {
        name,
        kind,
        multi: false,
        required: false,
        case_exact: false,
        mutability: "readWrite",
        uniqueness: "none",
        sub: Vec::new(),
    }
}

impl A {
    fn multi(mut self) -> Self {
        self.multi = true;
        self
    }
    fn required(mut self) -> Self {
        self.required = true;
        self
    }
    fn exact(mut self) -> Self {
        self.case_exact = true;
        self
    }
    fn read_only(mut self) -> Self {
        self.mutability = "readOnly";
        self
    }
    fn unique(mut self, u: &'static str) -> Self {
        self.uniqueness = u;
        self
    }
    fn subs(mut self, s: Vec<A>) -> Self {
        self.sub = s;
        self
    }
    fn json(&self) -> Value {
        let mut v = json!({
            "name": self.name,
            "type": self.kind,
            "multiValued": self.multi,
            "required": self.required,
            "caseExact": self.case_exact,
            "mutability": self.mutability,
            "returned": "default",
            "uniqueness": self.uniqueness,
        });
        if !self.sub.is_empty() {
            v["subAttributes"] = Value::Array(self.sub.iter().map(A::json).collect());
        }
        v
    }
}

fn schema(id: &str, name: &str, description: &str, attrs: Vec<A>, base: &str) -> Value {
    json!({
        "schemas": [SCHEMA_SCHEMA],
        "id": id,
        "name": name,
        "description": description,
        "attributes": attrs.iter().map(A::json).collect::<Vec<_>>(),
        "meta": {"resourceType": "Schema", "location": format!("{base}/Schemas/{id}")}
    })
}

pub fn user_schema(base: &str) -> Value {
    schema(
        USER_SCHEMA,
        "User",
        "User account (attributes not listed are accepted and ignored; password is never stored)",
        vec![
            a("userName", "string").required().unique("server"),
            a("externalId", "string").exact(),
            a("name", "complex").subs(vec![
                a("givenName", "string"),
                a("familyName", "string"),
                a("formatted", "string"),
            ]),
            a("displayName", "string"),
            a("active", "boolean"),
            a("emails", "complex").multi().subs(vec![
                a("value", "string"),
                a("type", "string"),
                a("primary", "boolean"),
            ]),
            a("groups", "complex").multi().read_only().subs(vec![
                a("value", "string").read_only(),
                a("display", "string").read_only(),
                a("$ref", "reference").read_only(),
            ]),
        ],
        base,
    )
}

pub fn group_schema(base: &str) -> Value {
    schema(
        GROUP_SCHEMA,
        "Group",
        "Group of SCIM users; mapped to roles by an administrator",
        vec![
            a("displayName", "string").required().unique("server"),
            a("externalId", "string").exact(),
            a("members", "complex").multi().subs(vec![
                a("value", "string").exact(),
                a("display", "string").read_only(),
                a("type", "string"),
                a("$ref", "reference"),
            ]),
        ],
        base,
    )
}

pub fn schemas(base: &str) -> Vec<Value> {
    vec![user_schema(base), group_schema(base)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_documents_describe_the_supported_features() {
        let spc = service_provider_config(&ScimConfig::default(), "/b");
        assert_eq!(spc["patch"]["supported"], true);
        assert_eq!(spc["bulk"]["supported"], false);
        assert_eq!(spc["filter"]["maxResults"], 200);
        assert_eq!(spc["authenticationSchemes"][0]["type"], "oauthbearertoken");
        let rts = resource_types("/b");
        assert_eq!(rts[0]["endpoint"], "/Users");
        let us = user_schema("/b");
        assert_eq!(us["attributes"][0]["name"], "userName");
        assert_eq!(us["attributes"][0]["uniqueness"], "server");
        assert_eq!(
            group_schema("/b")["attributes"][2]["subAttributes"][0]["name"],
            "value"
        );
    }
}
