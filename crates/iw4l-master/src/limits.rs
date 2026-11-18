use master_protocol::budget::TransportBudget;
use std::sync::OnceLock;

pub(crate) fn configured() -> Result<TransportBudget, String> {
    static BUDGET: OnceLock<Result<TransportBudget, String>> = OnceLock::new();
    BUDGET
        .get_or_init(|| {
            TransportBudget::from_settings(TransportBudget::SETTINGS.into_iter().filter_map(
                |name| {
                    std::env::var(name)
                        .ok()
                        .map(|value| (name.to_owned(), value))
                },
            ))
        })
        .clone()
}

pub(crate) fn values() -> TransportBudget {
    configured().unwrap_or_else(|_| TransportBudget::default())
}
