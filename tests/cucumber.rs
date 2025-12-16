//! Cucumber BDD test runner

use cucumber::World;

mod features;

use features::TestWorld;

#[tokio::main]
async fn main() {
    TestWorld::run("tests/features").await;
}
