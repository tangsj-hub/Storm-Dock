use serde_json::Value;

use crate::error::Result;
use crate::models::{
    now, parse_iso_timestamp, Account, CursorUsageDetails, ProductUsageShare, UsageEvent,
    UsageMetric, WeeklyUsageSummary,
};

pub(crate) fn usage_from_billing(
    account: &Account,
    raw: &Value,
    membership: Option<String>,
) -> Result<CursorUsageDetails> {
    let config = raw.get("config").cloned().unwrap_or_else(|| raw.clone());
    let history = config
        .get("history")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let period_end = text_at(&config, "currentPeriod", "end")
        .or_else(|| text_val(&config, &["billingPeriodEnd", "billing_period_end"]));

    let products = product_shares(&config);
    // Official meter is the combined pool (`creditUsagePercent` = Build + Imagine + …).
    // Do not treat omitted percent as 0% (that bug shows a fake empty bar).
    let primary = f64_val(&config, "creditUsagePercent")
        .or_else(|| {
            products
                .iter()
                .find(|item| item.name == "Grok Build")
                .map(|item| item.percent)
        })
        .map(percent_metric)
        .or_else(|| {
            let used = money_val(&config, "used")?;
            Some(currency_from_cents(
                used,
                money_val(&config, "monthlyLimit"),
            ))
        })
        .unwrap_or_else(|| percent_metric(0.0));

    let on_demand_used = money_val(&config, "onDemandUsed").or_else(|| {
        history
            .first()
            .and_then(|item| money_val(item, "onDemandUsed"))
    });
    let on_demand_cap = money_val(&config, "onDemandCap");
    let on_demand = match (on_demand_used, on_demand_cap) {
        (Some(used), Some(limit)) if limit > 0.0 || used > 0.0 => {
            Some(currency_from_cents(used, Some(limit)))
        }
        (Some(used), None) if used > 0.0 => Some(currency_from_cents(used, None)),
        (None, Some(limit)) if limit > 0.0 => Some(currency_from_cents(0.0, Some(limit))),
        _ => None,
    };

    let mut weekly = Vec::new();
    let mut events = Vec::new();
    for item in &history {
        let Some((date, timestamp)) = history_point(item) else {
            continue;
        };
        let included = money_val(item, "includedUsed").unwrap_or(0.0);
        let on_demand_used = money_val(item, "onDemandUsed").unwrap_or(0.0);
        let total = money_val(item, "totalUsed").unwrap_or(included + on_demand_used);
        weekly.push(WeeklyUsageSummary {
            date: date.clone(),
            requests: 0.0,
            on_demand_cents: on_demand_used,
            is_on_demand: on_demand_used > 0.0,
        });
        if total > 0.0 || on_demand_used > 0.0 {
            events.push(UsageEvent {
                timestamp,
                model: Some(
                    item.get("period")
                        .and_then(|period| text_val(period, &["type", "periodType"]))
                        .unwrap_or(date),
                ),
                requests: 0.0,
                input_tokens: None,
                output_tokens: None,
                cost_usd: Some(total / 100.0),
                charged_cents: Some(total),
                on_demand: on_demand_used > 0.0,
            });
        }
    }
    weekly.reverse();
    events.reverse();

    Ok(CursorUsageDetails {
        account_id: account.id.clone(),
        label: account.label.clone(),
        email: account.email.clone(),
        name: None,
        membership_type: membership.or_else(|| account.subscription.plan.clone()),
        primary,
        reset_at: period_end,
        on_demand,
        grok_bot: None,
        grok_bot_reset_at: None,
        products,
        breakdown: None,
        models: Vec::new(),
        weekly,
        weekly_available: true,
        weekly_error: None,
        events,
        checked_at: now(),
    })
}

fn product_shares(config: &Value) -> Vec<ProductUsageShare> {
    config
        .get("productUsage")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|product| {
            let percent =
                f64_val(product, "usagePercent").or_else(|| f64_val(product, "usage_percent"))?;
            Some(ProductUsageShare {
                name: product_label(product.get("product")?.as_str()?),
                percent,
            })
        })
        .collect()
}

fn product_label(raw: &str) -> String {
    match raw
        .trim()
        .replace(['_', '-'], "")
        .to_ascii_lowercase()
        .as_str()
    {
        "grokbuild" => "Grok Build".into(),
        "grokimagine" => "Imagine".into(),
        "grokchat" => "Grok Chat".into(),
        _ => raw.trim().to_owned(),
    }
}

fn percent_metric(percent: f64) -> UsageMetric {
    UsageMetric {
        kind: "percent".into(),
        used: percent,
        limit: Some(100.0),
        percent,
    }
}

fn currency_from_cents(used: f64, limit: Option<f64>) -> UsageMetric {
    let percent = match limit {
        Some(limit) if limit > 0.0 => (used / limit) * 100.0,
        _ => 0.0,
    };
    UsageMetric {
        kind: "currency".into(),
        used,
        limit,
        percent,
    }
}

fn history_point(item: &Value) -> Option<(String, u64)> {
    if let Some(cycle) = item.get("billingCycle") {
        let year = cycle.get("year").and_then(Value::as_u64)?;
        let month = cycle.get("month").and_then(Value::as_u64)?;
        let date = format!("{year:04}-{month:02}-01");
        return Some((date, rough_month_timestamp(year, month)));
    }
    let period = item.get("period")?;
    let stamp = text_val(period, &["start", "end"])?;
    let date = iso_date(&stamp)?;
    let timestamp = parse_iso_timestamp(&stamp).unwrap_or_else(|| rough_from_date(&date));
    Some((date, timestamp))
}

fn iso_date(text: &str) -> Option<String> {
    let text = text.trim();
    (text.len() >= 10 && text.as_bytes().get(4) == Some(&b'-')).then(|| text[..10].to_owned())
}

fn rough_from_date(date: &str) -> u64 {
    let year = date
        .get(..4)
        .and_then(|value| value.parse().ok())
        .unwrap_or(1970);
    let month = date
        .get(5..7)
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    rough_month_timestamp(year, month)
}

fn rough_month_timestamp(year: u64, month: u64) -> u64 {
    let month = month.clamp(1, 12);
    (year.saturating_sub(1970) * 365 + (month - 1) * 30 + 15) * 86_400
}

fn money_val(value: &Value, key: &str) -> Option<f64> {
    f64_val(value, key)
}

fn f64_val(value: &Value, key: &str) -> Option<f64> {
    let node = value.get(key)?;
    if let Some(number) = node.as_f64().or_else(|| node.as_i64().map(|n| n as f64)) {
        return Some(number);
    }
    node.get("val")
        .and_then(|val| val.as_f64().or_else(|| val.as_i64().map(|n| n as f64)))
}

fn text_val(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn text_at(value: &Value, object: &str, key: &str) -> Option<String> {
    value.get(object).and_then(|child| text_val(child, &[key]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ApplicationKind, ImportType, SubscriptionSummary};

    fn sample_account() -> Account {
        Account {
            id: "acc_test".into(),
            label: "Grok".into(),
            email: Some("a@b.com".into()),
            application: ApplicationKind::Grok,
            import_type: ImportType::OAuth,
            subscription: SubscriptionSummary::default(),
            raw_export: serde_json::json!({}),
            created_at: 1,
            updated_at: 1,
            last_used_at: 1,
        }
    }

    #[test]
    fn maps_credits_weekly_percent() {
        let raw = serde_json::json!({
            "config": {
                "currentPeriod": {
                    "type": "USAGE_PERIOD_TYPE_WEEKLY",
                    "start": "2026-08-08T01:53:09.930537+00:00",
                    "end": "2026-08-15T01:53:09.930537+00:00"
                },
                "creditUsagePercent": 75.0,
                "onDemandCap": {"val": 0},
                "onDemandUsed": {"val": 0},
                "productUsage": [
                    {"product": "GrokBuild", "usagePercent": 61.2},
                    {"product": "GrokChat"}
                ],
                "billingPeriodEnd": "2026-08-15T01:53:09.930537+00:00"
            }
        });
        let details =
            usage_from_billing(&sample_account(), &raw, Some("supergrok".into())).unwrap();
        assert_eq!(details.primary.kind, "percent");
        assert_eq!(details.primary.percent, 75.0);
        assert!(details.on_demand.is_none());
        assert_eq!(
            details.reset_at.as_deref(),
            Some("2026-08-15T01:53:09.930537+00:00")
        );
    }

    #[test]
    fn maps_legacy_cent_payload() {
        let raw = serde_json::json!({
            "config": {
                "monthlyLimit": {"val": 2000},
                "used": {"val": 1234},
                "onDemandCap": {"val": 500},
                "billingPeriodEnd": "2026-10-01T00:00:00+00:00",
                "history": [{
                    "billingCycle": {"year": 2026, "month": 9},
                    "includedUsed": {"val": 1234},
                    "onDemandUsed": {"val": 100},
                    "totalUsed": {"val": 1334}
                }]
            }
        });
        let details = usage_from_billing(&sample_account(), &raw, Some("free".into())).unwrap();
        assert_eq!(details.primary.kind, "currency");
        assert_eq!(details.primary.used, 1234.0);
        assert_eq!(details.primary.limit, Some(2000.0));
        assert_eq!(
            details.on_demand.as_ref().map(|item| item.used),
            Some(100.0)
        );
        assert_eq!(details.weekly.len(), 1);
        assert_eq!(
            details.reset_at.as_deref(),
            Some("2026-10-01T00:00:00+00:00")
        );
    }

    #[test]
    fn maps_credits_history_period() {
        let raw = serde_json::json!({
            "config": {
                "creditUsagePercent": 10.0,
                "history": [{
                    "period": {
                        "type": "USAGE_PERIOD_TYPE_WEEKLY",
                        "start": "2026-05-25T00:00:00Z",
                        "end": "2026-06-01T00:00:00Z"
                    },
                    "onDemandUsed": {"val": 120}
                }]
            }
        });
        let details = usage_from_billing(&sample_account(), &raw, None).unwrap();
        assert_eq!(details.weekly[0].date, "2026-05-25");
        assert_eq!(details.weekly[0].on_demand_cents, 120.0);
        assert_eq!(details.events[0].charged_cents, Some(120.0));
    }

    #[test]
    fn official_percent_includes_imagine() {
        let raw = serde_json::json!({
            "config": {
                "creditUsagePercent": 100.0,
                "productUsage": [
                    {"product": "GrokBuild", "usagePercent": 96.0},
                    {"product": "GrokImagine", "usagePercent": 4.0}
                ]
            }
        });
        let details = usage_from_billing(&sample_account(), &raw, None).unwrap();
        assert_eq!(details.primary.percent, 100.0);
        assert_eq!(details.products.len(), 2);
        assert_eq!(details.products[0].name, "Grok Build");
        assert_eq!(details.products[0].percent, 96.0);
        assert_eq!(details.products[1].name, "Imagine");
        assert_eq!(details.products[1].percent, 4.0);
    }

    #[test]
    fn zero_on_demand_cap_is_not_a_quota_bar() {
        let raw = serde_json::json!({
            "config": {
                "creditUsagePercent": 40.0,
                "onDemandCap": {"val": 0},
                "onDemandUsed": {"val": 0}
            }
        });
        let details = usage_from_billing(&sample_account(), &raw, None).unwrap();
        assert!(details.on_demand.is_none());
        assert_eq!(details.primary.percent, 40.0);
    }
}
