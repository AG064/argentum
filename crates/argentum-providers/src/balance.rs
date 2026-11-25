// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! The account balance of providers that report one: DeepSeek and
//! OpenRouter.
//!
//! The balance is read only when the owner asks for it, from the provider's
//! own read-only endpoint on the provider's own host, with the key the owner
//! configured for that provider. The key goes only into the request's
//! authorization header; it is never logged, stored or returned.

use argentum_domain::AccountBalance;
use url::Url;

use super::ProviderError;

/// Where a provider reports its balance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BalanceRoute {
    /// `GET https://api.deepseek.com/user/balance`.
    DeepSeek,
    /// `GET https://openrouter.ai/api/v1/credits`.
    OpenRouter,
}

impl BalanceRoute {
    /// The route for a provider endpoint, by its host. Other providers
    /// report no balance.
    pub(crate) fn for_endpoint(endpoint: &Url) -> Option<Self> {
        if endpoint.scheme() != "https" {
            return None;
        }
        match endpoint.host_str()? {
            "api.deepseek.com" => Some(Self::DeepSeek),
            "openrouter.ai" => Some(Self::OpenRouter),
            _ => None,
        }
    }

    /// The balance URL on the same host as the endpoint.
    pub(crate) fn url(self, endpoint: &Url) -> Result<Url, ProviderError> {
        Ok(match self {
            Self::DeepSeek => endpoint.join("/user/balance")?,
            Self::OpenRouter => endpoint.join("/api/v1/credits")?,
        })
    }

    /// The balance from the provider's JSON reply.
    pub(crate) fn parse(self, body: &[u8]) -> Result<AccountBalance, ProviderError> {
        let invalid =
            || ProviderError::InvalidResponse("balance reply has an unknown shape".into());
        let value: serde_json::Value = serde_json::from_slice(body).map_err(|_| invalid())?;
        match self {
            Self::DeepSeek => {
                let info = value
                    .get("balance_infos")
                    .and_then(serde_json::Value::as_array)
                    .and_then(|infos| infos.first())
                    .ok_or_else(invalid)?;
                let text = |key: &str| info.get(key).and_then(serde_json::Value::as_str);
                let currency = text("currency").ok_or_else(invalid)?;
                let total = text("total_balance").ok_or_else(invalid)?;
                Ok(AccountBalance {
                    remaining: format!("{} {}", bounded(total), bounded(currency)),
                    detail: if value.get("is_available") == Some(&serde_json::Value::Bool(false)) {
                        "The provider says the balance is not enough to send requests.".into()
                    } else {
                        String::new()
                    },
                })
            }
            Self::OpenRouter => {
                let data = value.get("data").ok_or_else(invalid)?;
                let number = |key: &str| data.get(key).and_then(serde_json::Value::as_f64);
                let total = number("total_credits").ok_or_else(invalid)?;
                let used = number("total_usage").ok_or_else(invalid)?;
                Ok(AccountBalance {
                    remaining: format!("{:.2} USD", total - used),
                    detail: format!("{used:.2} of {total:.2} USD in credits used"),
                })
            }
        }
    }
}

/// Provider text shown to the owner: short and on one line.
fn bounded(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_deepseek_and_openrouter_over_https_have_a_balance() {
        let url = |text: &str| Url::parse(text).unwrap();
        assert_eq!(
            BalanceRoute::for_endpoint(&url("https://api.deepseek.com/")),
            Some(BalanceRoute::DeepSeek)
        );
        assert_eq!(
            BalanceRoute::for_endpoint(&url("https://openrouter.ai/api/v1/")),
            Some(BalanceRoute::OpenRouter)
        );
        assert_eq!(
            BalanceRoute::for_endpoint(&url("http://api.deepseek.com/")),
            None
        );
        assert_eq!(
            BalanceRoute::for_endpoint(&url("https://api.openai.com/v1/")),
            None
        );
        assert_eq!(
            BalanceRoute::DeepSeek
                .url(&url("https://api.deepseek.com/v1/"))
                .unwrap()
                .as_str(),
            "https://api.deepseek.com/user/balance"
        );
        assert_eq!(
            BalanceRoute::OpenRouter
                .url(&url("https://openrouter.ai/api/v1/"))
                .unwrap()
                .as_str(),
            "https://openrouter.ai/api/v1/credits"
        );
    }

    #[test]
    fn replies_are_read_or_refused() {
        let deepseek = br#"{"is_available":true,"balance_infos":[{"currency":"USD","total_balance":"110.00","granted_balance":"10.00","topped_up_balance":"100.00"}]}"#;
        let balance = BalanceRoute::DeepSeek.parse(deepseek).unwrap();
        assert_eq!(balance.remaining, "110.00 USD");
        assert!(balance.detail.is_empty());
        let empty = br#"{"is_available":false,"balance_infos":[{"currency":"CNY","total_balance":"0.00"}]}"#;
        assert!(!BalanceRoute::DeepSeek
            .parse(empty)
            .unwrap()
            .detail
            .is_empty());
        let openrouter = br#"{"data":{"total_credits":25.5,"total_usage":3.25}}"#;
        let balance = BalanceRoute::OpenRouter.parse(openrouter).unwrap();
        assert_eq!(balance.remaining, "22.25 USD");
        assert_eq!(balance.detail, "3.25 of 25.50 USD in credits used");
        assert!(BalanceRoute::DeepSeek.parse(b"{}").is_err());
        assert!(BalanceRoute::OpenRouter.parse(b"not json").is_err());
    }
}
