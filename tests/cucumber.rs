//! Cucumber BDD test runner

use cucumber::World;

mod features;

use features::TestWorld;

#[tokio::main]
async fn main() {
    // Run cucumber tests, skipping @wip (work in progress) features
    // @wip features are those that test functionality not yet implemented
    TestWorld::cucumber()
        .filter_run("tests/features", |feature, _rule, _scenario| {
            // Skip features tagged with @wip
            !feature.tags.iter().any(|tag| tag == "wip")
        })
        .await;
}
