#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", target_os = "linux"))]

use tokio::task::coop::{consume_budget, has_budget_remaining};

const BUDGET: usize = 128;

#[tokio::test]
async fn test_has_budget_remaining() {
    // At the beginning, budget should be available.
    assert!(has_budget_remaining());

    // Deplete the budget
    for _ in 0..BUDGET {
        consume_budget().await;
    }

    assert!(!has_budget_remaining());
}
