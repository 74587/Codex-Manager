use codexmanager_core::storage::ModelFastPolicyV2;
use serde_json::Value;

pub(crate) const FAST_REQUEST_BLOCKED: &str = "fast_request_blocked";

pub(crate) fn apply(
    body: Vec<u8>,
    policy: ModelFastPolicyV2,
    client_service_tier: Option<&str>,
) -> Result<(Vec<u8>, bool), &'static str> {
    if policy == ModelFastPolicyV2::Block && is_fast_request_tier(client_service_tier) {
        return Err(FAST_REQUEST_BLOCKED);
    }
    if matches!(
        policy,
        ModelFastPolicyV2::Passthrough | ModelFastPolicyV2::Block
    ) {
        return Ok((body, false));
    }

    let Ok(mut payload) = serde_json::from_slice::<Value>(&body) else {
        return Ok((body, false));
    };
    let Some(object) = payload.as_object_mut() else {
        return Ok((body, false));
    };
    let changed = match policy {
        ModelFastPolicyV2::Filter => object.remove("service_tier").is_some(),
        ModelFastPolicyV2::Force => {
            object.insert(
                "service_tier".to_string(),
                Value::String("priority".to_string()),
            );
            true
        }
        ModelFastPolicyV2::Passthrough | ModelFastPolicyV2::Block => false,
    };
    if !changed {
        return Ok((body, false));
    }
    Ok((serde_json::to_vec(&payload).unwrap_or(body), true))
}

fn is_fast_request_tier(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "fast" | "priority"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service_tier(body: &[u8]) -> Option<String> {
        serde_json::from_slice::<Value>(body)
            .ok()?
            .get("service_tier")?
            .as_str()
            .map(str::to_string)
    }

    #[test]
    fn passthrough_preserves_service_tier() {
        let body = br#"{"service_tier":"ultrafast"}"#.to_vec();
        let (body, applied) =
            apply(body, ModelFastPolicyV2::Passthrough, Some("ultrafast")).unwrap();
        assert!(!applied);
        assert_eq!(service_tier(&body).as_deref(), Some("ultrafast"));
    }

    #[test]
    fn filter_removes_service_tier() {
        let body = br#"{"service_tier":"fast","input":[]}"#.to_vec();
        let (body, applied) = apply(body, ModelFastPolicyV2::Filter, Some("fast")).unwrap();
        assert!(applied);
        assert_eq!(service_tier(&body), None);
    }

    #[test]
    fn force_sets_priority() {
        let body = br#"{"input":[]}"#.to_vec();
        let (body, applied) = apply(body, ModelFastPolicyV2::Force, None).unwrap();
        assert!(applied);
        assert_eq!(service_tier(&body).as_deref(), Some("priority"));
    }

    #[test]
    fn block_only_rejects_tiers_that_request_fast_processing() {
        for tier in ["fast", "priority", " FAST "] {
            let body = serde_json::to_vec(&serde_json::json!({ "service_tier": tier })).unwrap();
            assert_eq!(
                apply(body, ModelFastPolicyV2::Block, Some(tier)),
                Err(FAST_REQUEST_BLOCKED),
                "tier {tier} must be blocked"
            );
        }

        for tier in [
            "auto",
            "default",
            "standard",
            "flex",
            "ultrafast",
            "invalid",
        ] {
            let body = serde_json::to_vec(&serde_json::json!({ "service_tier": tier })).unwrap();
            let (body, applied) =
                apply(body, ModelFastPolicyV2::Block, Some(tier)).expect("non-Fast tier allowed");
            assert!(!applied);
            assert_eq!(service_tier(&body).as_deref(), Some(tier));
        }

        let body = br#"{"service_tier":"priority"}"#.to_vec();
        let (body, applied) = apply(body, ModelFastPolicyV2::Block, None).unwrap();
        assert!(!applied);
        assert_eq!(service_tier(&body).as_deref(), Some("priority"));
    }
}
