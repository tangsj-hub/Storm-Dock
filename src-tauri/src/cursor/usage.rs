use std::collections::BTreeMap;
use time::{Date, Duration as TimeDuration, OffsetDateTime};

use crate::cursor::api::{dashboard_cookie, dashboard_request};
use crate::error::Result;
use crate::models::{
    now, Account, CursorUsageDetails, ModelUsageSummary, Session, UsageEvent, UsageMetric,
    WeeklyUsageSummary,
};

const USAGE_EVENTS_PAGE_SIZE: u32 = 100;
const USAGE_EVENTS_MAX_PAGES: u32 = 5;

pub(crate) fn number_at(value: &serde_json::Value, path: &[&str]) -> Option<f64> {
    path.iter()
        .try_fold(value, |value, key| value.get(*key))
        .and_then(json_number)
}

pub(crate) fn json_number(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_u64().map(|number| number as f64))
        .or_else(|| value.as_i64().map(|number| number as f64))
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        .filter(|number| number.is_finite())
}

pub(crate) fn json_i64(value: &serde_json::Value) -> Option<i64> {
    json_number(value).and_then(|number| {
        (number.fract() == 0.0 && (i64::MIN as f64..=i64::MAX as f64).contains(&number))
            .then_some(number as i64)
    })
}

pub(crate) fn normalized_email(value: &str) -> Option<String> {
    let email = value.trim().to_ascii_lowercase();
    (!email.is_empty()).then_some(email)
}

pub(crate) fn first_team_id(teams: &serde_json::Value) -> Option<i64> {
    teams
        .get("teams")
        .and_then(serde_json::Value::as_array)
        .or_else(|| teams.as_array())
        .into_iter()
        .flatten()
        .find_map(|team| {
            team.get("id")
                .or_else(|| team.get("teamId"))
                .and_then(json_i64)
        })
}

pub(crate) fn team_member_user_id(spend: &serde_json::Value, emails: &[String]) -> Option<i64> {
    let wanted: Vec<String> = emails
        .iter()
        .filter_map(|email| normalized_email(email))
        .collect();
    if wanted.is_empty() {
        return None;
    }
    spend
        .get("teamMemberSpend")
        .or_else(|| spend.pointer("/data/teamMemberSpend"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|member| {
            let email = ["email", "userEmail", "cursorEmail"]
                .iter()
                .find_map(|key| member.get(*key).and_then(serde_json::Value::as_str))
                .and_then(normalized_email)?;
            if !wanted.iter().any(|value| value == &email) {
                return None;
            }
            member
                .get("userId")
                .or_else(|| member.get("id"))
                .and_then(json_i64)
        })
}

pub(crate) fn event_timestamp_ms(event: &serde_json::Value) -> Option<u64> {
    let value = event.get("timestamp")?;
    let number = json_i64(value)
        .map(|number| number as u64)
        .or_else(|| value.as_str()?.parse::<u64>().ok())?;
    Some(if number > 10_000_000_000 {
        number
    } else {
        number * 1000
    })
}

pub(crate) fn event_date(event: &serde_json::Value) -> Option<Date> {
    let ms = event_timestamp_ms(event)? as i128;
    OffsetDateTime::from_unix_timestamp_nanos(ms * 1_000_000)
        .ok()
        .map(|time| time.date())
}

pub(crate) fn events_range_ms() -> (String, String) {
    let end = OffsetDateTime::now_utc();
    let start = end - TimeDuration::days(7);
    (
        (start.unix_timestamp_nanos() / 1_000_000).to_string(),
        (end.unix_timestamp_nanos() / 1_000_000).to_string(),
    )
}

pub(crate) fn auth_numeric_id(me: &serde_json::Value) -> Option<i64> {
    ["id", "userId", "user_id"]
        .iter()
        .find_map(|key| me.get(*key).and_then(json_i64))
}

pub(crate) fn usage_events_from_response(
    value: &serde_json::Value,
) -> Option<Vec<serde_json::Value>> {
    let item = ["usageEventsDisplay", "usageEvents", "events"]
        .iter()
        .find_map(|key| value.get(*key))
        .or_else(|| value.pointer("/data/usageEventsDisplay"));
    match item {
        None | Some(serde_json::Value::Null) => Some(Vec::new()),
        Some(serde_json::Value::Array(items)) => Some(items.clone()),
        _ => None,
    }
}

pub(crate) fn usage_events_body(
    team_id: Option<i64>,
    user_id: Option<i64>,
    page: u32,
    dated: bool,
) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    body.insert("page".into(), serde_json::Value::from(page));
    body.insert(
        "pageSize".into(),
        serde_json::Value::from(USAGE_EVENTS_PAGE_SIZE),
    );
    if let Some(team_id) = team_id {
        body.insert("teamId".into(), serde_json::Value::from(team_id));
    }
    if let Some(user_id) = user_id {
        body.insert("userId".into(), serde_json::Value::from(user_id));
    }
    if dated {
        let (start, end) = events_range_ms();
        body.insert("startDate".into(), serde_json::Value::String(start));
        body.insert("endDate".into(), serde_json::Value::String(end));
    }
    serde_json::Value::Object(body)
}

pub(crate) fn collect_usage_event_pages(
    cookie: &str,
    team_id: Option<i64>,
    user_id: Option<i64>,
    dated: bool,
) -> Result<Vec<serde_json::Value>> {
    let cutoff = OffsetDateTime::now_utc().date() - TimeDuration::days(6);
    let mut collected = Vec::new();
    for page in 1..=USAGE_EVENTS_MAX_PAGES {
        let response = dashboard_request(
            cookie,
            "/dashboard/get-filtered-usage-events",
            Some(usage_events_body(team_id, user_id, page, dated)),
        )?;
        let Some(events) = usage_events_from_response(&response) else {
            break;
        };
        if events.is_empty() {
            break;
        }
        let oldest = events.iter().filter_map(event_date).min();
        let page_len = events.len();
        collected.extend(events);
        if oldest.is_some_and(|date| date < cutoff) || page_len < USAGE_EVENTS_PAGE_SIZE as usize {
            break;
        }
    }
    Ok(collected)
}

pub(crate) fn collect_usage_events(
    cookie: &str,
    team_id: Option<i64>,
    user_id: Option<i64>,
) -> Result<serde_json::Value> {
    // Personal accounts: CursorMeter sends only teamId 0 / page / pageSize.
    // Dated bodies are used by the dashboard, but free plans often reply without
    // usageEventsDisplay; treat that as empty and retry the undated shape.
    let personal = team_id == Some(0) && user_id.is_none();
    let shapes = if personal {
        vec![(Some(0), None, false), (None, None, false)]
    } else {
        vec![(team_id, user_id, false), (team_id, user_id, true)]
    };
    let mut collected = Vec::new();
    let mut last_error = None;
    for (next_team_id, next_user_id, dated) in shapes {
        match collect_usage_event_pages(cookie, next_team_id, next_user_id, dated) {
            Ok(events) => {
                collected = events;
                if !collected.is_empty() {
                    break;
                }
            }
            Err(error) => last_error = Some(error),
        }
    }
    if collected.is_empty() {
        if let Some(error) = last_error {
            return Err(error);
        }
    }
    Ok(serde_json::json!({ "usageEventsDisplay": collected }))
}

pub(crate) fn text_at(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    path.iter()
        .try_fold(value, |value, key| value.get(*key))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

pub(crate) fn json_model_name(value: &serde_json::Value) -> Option<String> {
    if let Some(name) = value
        .as_str()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        return Some(name.to_owned());
    }
    ["name", "model", "modelName", "displayName"]
        .iter()
        .find_map(|key| {
            value
                .get(*key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
        })
}

pub(crate) fn event_model_name(event: &serde_json::Value) -> Option<String> {
    ["model", "modelName", "model_name"]
        .iter()
        .find_map(|key| event.get(*key).and_then(json_model_name))
}

pub(crate) fn event_request_weight(event: &serde_json::Value) -> f64 {
    event
        .get("requestsCosts")
        .and_then(json_number)
        .map(|value| value.max(0.0))
        .unwrap_or(0.0)
}

pub(crate) fn event_number(event: &serde_json::Value, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| event.get(*key).and_then(json_number))
        .filter(|value| value.is_finite())
}

pub(crate) fn usage_event_from_value(event: &serde_json::Value) -> Option<UsageEvent> {
    Some(UsageEvent {
        timestamp: event_timestamp_ms(event)?,
        model: event_model_name(event),
        requests: event_request_weight(event),
        input_tokens: event_number(event, &["inputTokens", "input_tokens", "inputTokenCount"]),
        output_tokens: event_number(
            event,
            &["outputTokens", "output_tokens", "outputTokenCount"],
        ),
        cost_usd: event_number(event, &["costUsd", "cost_usd", "costUSD"]),
        charged_cents: event_number(event, &["chargedCents", "charged_cents"]),
        on_demand: event.get("kind").and_then(serde_json::Value::as_str)
            == Some("USAGE_EVENT_KIND_USAGE_BASED"),
    })
}

pub(crate) fn usage_event_rows(events: &serde_json::Value) -> Vec<UsageEvent> {
    let mut rows: Vec<_> = events
        .get("usageEventsDisplay")
        .or_else(|| events.get("usageEvents"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(usage_event_from_value)
        .collect();
    rows.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
    rows
}

pub(crate) fn sort_models(models: &mut [ModelUsageSummary]) {
    models.sort_by(|left, right| {
        right
            .requests
            .cmp(&left.requests)
            .then_with(|| left.name.cmp(&right.name))
    });
}

pub(crate) fn models_from_events(events: &serde_json::Value) -> Vec<ModelUsageSummary> {
    let mut by_model: BTreeMap<String, f64> = BTreeMap::new();
    for event in events
        .get("usageEventsDisplay")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = event_model_name(event) else {
            continue;
        };
        *by_model.entry(name).or_default() += event_request_weight(event);
    }
    let mut models: Vec<_> = by_model
        .into_iter()
        .filter(|(_, requests)| *requests > 0.0)
        .map(|(name, requests)| ModelUsageSummary {
            name,
            requests: requests.round() as u64,
        })
        .collect();
    sort_models(&mut models);
    models
}

pub(crate) fn usage_metric(kind: &str, used: f64, limit: Option<f64>) -> UsageMetric {
    let percent = limit
        .filter(|limit| *limit > 0.0)
        .map(|limit| used / limit * 100.0)
        .unwrap_or(0.0);
    UsageMetric {
        kind: kind.into(),
        used,
        limit,
        percent,
    }
}

pub(crate) fn weekly_usage(events: &serde_json::Value) -> Option<Vec<WeeklyUsageSummary>> {
    let today = OffsetDateTime::now_utc().date();
    let days: Vec<Date> = (0..7)
        .rev()
        .map(|day| today - TimeDuration::days(day))
        .collect();
    let mut values: BTreeMap<Date, (f64, f64, bool)> = BTreeMap::new();
    for event in events.get("usageEventsDisplay")?.as_array()? {
        let Some(date) = event_date(event) else {
            continue;
        };
        if !days.contains(&date) {
            continue;
        }
        let item = values.entry(date).or_default();
        item.0 += event
            .get("requestsCosts")
            .and_then(json_number)
            .unwrap_or(0.0);
        if event.get("kind").and_then(serde_json::Value::as_str)
            == Some("USAGE_EVENT_KIND_USAGE_BASED")
        {
            item.1 += event
                .get("chargedCents")
                .and_then(json_number)
                .unwrap_or(0.0);
            item.2 = true;
        }
    }
    Some(
        days.into_iter()
            .map(|date| {
                let (requests, on_demand_cents, is_on_demand) =
                    values.get(&date).copied().unwrap_or_default();
                WeeklyUsageSummary {
                    date: date.to_string(),
                    requests,
                    on_demand_cents,
                    is_on_demand,
                }
            })
            .collect(),
    )
}

pub(crate) fn percent_from_message(text: Option<&str>) -> Option<f64> {
    let text = text?;
    let end = text.find('%')?;
    let head = &text[..end];
    let start = head
        .rfind(|character: char| !(character.is_ascii_digit() || character == '.'))
        .map(|index| index + 1)
        .unwrap_or(0);
    head.get(start..)?
        .parse()
        .ok()
        .filter(|value: &f64| value.is_finite())
}

pub(crate) fn percent_metric(percent: f64) -> UsageMetric {
    UsageMetric {
        kind: "percent".into(),
        used: percent,
        limit: None,
        percent,
    }
}

pub(crate) fn with_percent(mut metric: UsageMetric, percent: Option<f64>) -> UsageMetric {
    if let Some(percent) = percent {
        metric.percent = percent;
    }
    metric
}

pub(crate) fn plan_breakdown_total(summary: &serde_json::Value) -> Option<f64> {
    number_at(summary, &["individualUsage", "plan", "breakdown", "total"])
        .filter(|value| *value > 0.0)
}

pub(crate) fn per_user_limit_cents(
    summary: &serde_json::Value,
    hard_limit: Option<&serde_json::Value>,
) -> Option<f64> {
    hard_limit
        .and_then(|value| number_at(value, &["perUserMonthlyLimitDollars"]))
        .or_else(|| number_at(summary, &["hard_limit", "perUserMonthlyLimitDollars"]))
        .or_else(|| number_at(summary, &["perUserMonthlyLimitDollars"]))
        .filter(|value| *value > 0.0)
        .map(|value| value * 100.0)
}

pub(crate) fn inferred_cursor_limit_cents(summary: &serde_json::Value) -> Option<f64> {
    let total = plan_breakdown_total(summary)?;
    let percent = number_at(summary, &["individualUsage", "plan", "totalPercentUsed"])?;
    if percent <= 0.0 {
        return None;
    }
    if percent < 99.5 {
        return Some((total / (percent / 100.0)).round());
    }
    // The API caps totalPercentUsed at 100 even when spend slightly exceeds the
    // seat cap, so total / 100% would echo the used amount ($181.72 / $181.72).
    let snapped = (total / 1000.0).floor() * 1000.0;
    (snapped > 0.0 && total - snapped < 1000.0).then_some(snapped)
}

pub(crate) fn on_demand_metric(summary: &serde_json::Value) -> Option<UsageMetric> {
    let used = number_at(summary, &["individualUsage", "onDemand", "used"])
        .or_else(|| number_at(summary, &["teamUsage", "onDemand", "used"]))?;
    let limit = number_at(summary, &["individualUsage", "onDemand", "limit"])
        .or_else(|| number_at(summary, &["teamUsage", "onDemand", "limit"]))
        .filter(|limit| *limit > 0.0);
    Some(usage_metric("currency", used, limit))
}

pub(crate) fn is_team_scoped(summary: &serde_json::Value) -> bool {
    let membership = text_at(summary, &["membershipType"]).unwrap_or_default();
    let limit_type = text_at(summary, &["limitType"]).unwrap_or_default();
    ["enterprise", "team", "teams", "business"]
        .iter()
        .any(|name| membership.eq_ignore_ascii_case(name))
        || limit_type.eq_ignore_ascii_case("team")
}

pub(crate) fn usage_pools(
    summary: &serde_json::Value,
    hard_limit: Option<&serde_json::Value>,
) -> (UsageMetric, Option<UsageMetric>) {
    let plan_used = number_at(summary, &["individualUsage", "plan", "used"]);
    let plan_limit =
        number_at(summary, &["individualUsage", "plan", "limit"]).filter(|limit| *limit > 0.0);
    let overall_used = number_at(summary, &["individualUsage", "overall", "used"]);
    let total_percent = number_at(summary, &["individualUsage", "plan", "totalPercentUsed"])
        .or_else(|| {
            percent_from_message(text_at(summary, &["autoModelSelectedDisplayMessage"]).as_deref())
        });
    let auto_percent =
        number_at(summary, &["individualUsage", "plan", "autoPercentUsed"]).or_else(|| {
            percent_from_message(text_at(summary, &["autoModelSelectedDisplayMessage"]).as_deref())
        });
    let api_percent =
        number_at(summary, &["individualUsage", "plan", "apiPercentUsed"]).or_else(|| {
            percent_from_message(text_at(summary, &["namedModelSelectedDisplayMessage"]).as_deref())
        });
    let seat_limit = per_user_limit_cents(summary, hard_limit)
        .or_else(|| {
            number_at(summary, &["individualUsage", "overall", "limit"])
                .filter(|limit| *limit > 0.0)
        })
        .or_else(|| {
            let inferred = inferred_cursor_limit_cents(summary)?;
            match plan_limit {
                Some(plan) if inferred <= plan + 1.0 => None,
                _ => Some(inferred),
            }
        });
    let cursor_used = plan_breakdown_total(summary)
        .or(overall_used)
        .or_else(|| match (seat_limit, total_percent) {
            (Some(limit), Some(percent)) => Some(limit * percent / 100.0),
            _ => None,
        })
        .or(plan_used);
    let two_pool = auto_percent.is_some()
        || api_percent.is_some()
        || matches!((seat_limit, plan_limit), (Some(seat), Some(plan)) if seat > plan + 1.0);
    let primary = if let Some(limit) = seat_limit {
        usage_metric("currency", cursor_used.unwrap_or(0.0), Some(limit))
    } else if two_pool {
        percent_metric(auto_percent.or(total_percent).unwrap_or(0.0))
    } else if let (Some(used), Some(limit)) = (plan_used, plan_limit) {
        usage_metric("currency", used, Some(limit))
    } else if let Some(used) = overall_used {
        usage_metric("currency", used, None)
    } else if let Some(percent) = total_percent {
        percent_metric(percent)
    } else {
        usage_metric("requests", 0.0, None)
    };
    let on_demand = if two_pool {
        if let (Some(used), Some(limit)) = (plan_used, plan_limit) {
            Some(with_percent(
                usage_metric("currency", used, Some(limit)),
                api_percent,
            ))
        } else if let Some(percent) = api_percent {
            Some(percent_metric(percent))
        } else {
            on_demand_metric(summary)
        }
    } else {
        on_demand_metric(summary)
    };
    (primary, on_demand)
}

pub(crate) fn update_export_usage(
    record: &mut serde_json::Value,
    raw: serde_json::Value,
    checked_at: u64,
) {
    let Some(record) = record.as_object_mut() else {
        return;
    };
    let mut compatibility = raw
        .get("usage_summary")
        .cloned()
        .unwrap_or_else(|| raw.clone());
    let Some(usage_raw) = compatibility.as_object_mut() else {
        return;
    };
    let summary = raw.get("usage_summary").unwrap_or(&raw);
    for key in [
        "cursor_usage_sources",
        "total_input_tokens",
        "total_output_tokens",
        "used_models",
        "billing_cycle_start",
        "billing_cycle_end",
    ] {
        record.remove(key);
    }
    record.insert(
        "usage_updated_at".into(),
        serde_json::Value::from(checked_at),
    );
    if let Some(value) = text_at(summary, &["membershipType"]) {
        record.insert("membership_type".into(), serde_json::Value::String(value));
    }
    let events = raw
        .get("usage_events")
        .and_then(|value| value.get("usageEventsDisplay"))
        .and_then(serde_json::Value::as_array);
    let mut input_total = None;
    let mut output_total = None;
    let mut by_model: BTreeMap<String, serde_json::Map<String, serde_json::Value>> =
        BTreeMap::new();
    for event in events.into_iter().flatten() {
        let number = |keys: &[&str]| {
            keys.iter()
                .find_map(|key| event.get(*key).and_then(serde_json::Value::as_f64))
        };
        if let Some(value) = number(&["inputTokens", "input_tokens", "inputTokenCount"]) {
            input_total = Some(input_total.unwrap_or(0.0) + value);
        }
        if let Some(value) = number(&["outputTokens", "output_tokens", "outputTokenCount"]) {
            output_total = Some(output_total.unwrap_or(0.0) + value);
        }
        let Some(name) = event_model_name(event) else {
            continue;
        };
        let model = by_model.entry(name.clone()).or_insert_with(|| {
            let mut model = serde_json::Map::new();
            model.insert("model_name".into(), serde_json::Value::String(name));
            model
        });
        let requests = model
            .get("num_requests")
            .and_then(json_number)
            .unwrap_or(0.0)
            + event_request_weight(event);
        model.insert(
            "num_requests".into(),
            serde_json::Value::from(requests.round() as u64),
        );
        for (target, keys) in [
            (
                "input_tokens",
                &["inputTokens", "input_tokens", "inputTokenCount"][..],
            ),
            (
                "output_tokens",
                &["outputTokens", "output_tokens", "outputTokenCount"][..],
            ),
            ("cost_usd", &["costUsd", "cost_usd", "costUSD"][..]),
        ] {
            if let Some(value) = number(keys) {
                let total = model
                    .get(target)
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0)
                    + value;
                model.insert(target.into(), serde_json::Value::from(total));
            }
        }
    }
    if let Some(value) = input_total {
        usage_raw.insert("total_input_tokens".into(), serde_json::Value::from(value));
    }
    if let Some(value) = output_total {
        usage_raw.insert("total_output_tokens".into(), serde_json::Value::from(value));
    }
    if !by_model.is_empty() {
        usage_raw.insert(
            "used_models".into(),
            serde_json::Value::Array(
                by_model
                    .into_values()
                    .map(serde_json::Value::Object)
                    .collect(),
            ),
        );
    }
    if let Some(value) = raw.get("hard_limit") {
        usage_raw.insert("hard_limit".into(), value.clone());
    }
    record.insert("cursor_usage_raw".into(), compatibility);
}

pub(crate) fn cursor_usage_from_snapshot(
    account: &Account,
    raw: &serde_json::Value,
) -> Option<CursorUsageDetails> {
    let summary = raw;
    let (primary, on_demand) = usage_pools(summary, summary.get("hard_limit"));
    let mut models: Vec<_> = raw
        .get("used_models")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| {
            let requests = model
                .get("num_requests")
                .and_then(json_number)
                .filter(|value| *value > 0.0)?;
            Some(ModelUsageSummary {
                name: model.get("model_name")?.as_str()?.into(),
                requests: requests.round() as u64,
            })
        })
        .collect();
    sort_models(&mut models);
    Some(CursorUsageDetails {
        account_id: account.id.clone(),
        label: account.label.clone(),
        email: account.email.clone(),
        name: None,
        membership_type: text_at(summary, &["membershipType"]),
        primary,
        reset_at: text_at(summary, &["billingCycleEnd"]),
        on_demand,
        models,
        weekly_available: false,
        weekly: vec![],
        weekly_error: None,
        events: usage_event_rows(raw),
        checked_at: account
            .raw_export
            .get("usage_updated_at")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(account.updated_at),
    })
}

pub(crate) fn fetch_cursor_usage(
    account: &Account,
    session: &Session,
) -> Result<(CursorUsageDetails, serde_json::Value)> {
    let cookie = dashboard_cookie(session)?;
    let me = dashboard_request(&cookie, "/auth/me", None)?;
    let summary = dashboard_request(&cookie, "/usage-summary", None)?;
    let usage = dashboard_request(&cookie, "/usage", None)?;
    let email = text_at(&me, &["email"]);
    let enterprise = summary
        .get("membershipType")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case("enterprise"));
    let team_scoped = is_team_scoped(&summary);
    let teams = team_scoped
        .then(|| dashboard_request(&cookie, "/dashboard/teams", Some(serde_json::json!({}))).ok())
        .flatten();
    let team_id = teams.as_ref().and_then(first_team_id);
    let hard_limit = team_id.and_then(|team_id| {
        dashboard_request(
            &cookie,
            "/dashboard/get-hard-limit",
            Some(serde_json::json!({ "teamId": team_id })),
        )
        .ok()
    });
    let (mut primary, on_demand) = usage_pools(&summary, hard_limit.as_ref());
    if primary.kind == "requests" {
        let requests = usage
            .as_object()
            .into_iter()
            .flat_map(|map| map.values())
            .filter_map(|item| {
                item.get("numRequests")
                    .and_then(json_number)
                    .map(|value| value.round() as u64)
            })
            .sum::<u64>();
        primary = usage_metric("requests", requests as f64, None);
    }

    let team_spend = team_id.and_then(|team_id| {
        dashboard_request(
            &cookie,
            "/dashboard/get-team-spend",
            Some(serde_json::json!({ "teamId": team_id })),
        )
        .ok()
    });
    let member_emails: Vec<String> = [email.clone(), account.email.clone()]
        .into_iter()
        .flatten()
        .collect();
    let member_id = team_spend
        .as_ref()
        .and_then(|spend| team_member_user_id(spend, &member_emails))
        .or_else(|| auth_numeric_id(&me));
    let events_team_id = if enterprise { team_id } else { Some(0) };
    let events_user_id = if enterprise { member_id } else { None };
    let events_result = collect_usage_events(&cookie, events_team_id, events_user_id);
    let weekly_error = events_result.as_ref().err().map(|error| error.to_string());
    let usage_events = events_result.ok();
    let weekly = usage_events.as_ref().and_then(weekly_usage);
    let models = models_from_events(usage_events.as_ref().unwrap_or(&serde_json::Value::Null));
    let events = usage_events
        .as_ref()
        .map(usage_event_rows)
        .unwrap_or_default();
    let details = CursorUsageDetails {
        account_id: account.id.clone(),
        label: account.label.clone(),
        email,
        name: text_at(&me, &["name"]),
        membership_type: text_at(&summary, &["membershipType"]),
        primary,
        reset_at: text_at(&summary, &["billingCycleEnd"]),
        on_demand,
        models,
        weekly_available: weekly.is_some(),
        weekly: weekly.unwrap_or_default(),
        weekly_error,
        events,
        checked_at: now(),
    };
    let mut raw = serde_json::Map::new();
    raw.insert("auth_me".into(), me);
    raw.insert("usage_summary".into(), summary);
    raw.insert("usage".into(), usage);
    raw.insert(
        "usage_events".into(),
        usage_events.unwrap_or(serde_json::Value::Null),
    );
    if team_scoped {
        raw.insert("teams".into(), teams.unwrap_or(serde_json::Value::Null));
        raw.insert(
            "hard_limit".into(),
            hard_limit.unwrap_or(serde_json::Value::Null),
        );
        raw.insert(
            "team_spend".into(),
            team_spend.unwrap_or(serde_json::Value::Null),
        );
    }
    Ok((details, serde_json::Value::Object(raw)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    #[test]
    fn weekly_usage_reads_numeric_timestamps() {
        let now_ms = OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
        let events = serde_json::json!({ "usageEventsDisplay": [
            { "timestamp": now_ms, "requestsCosts": "1.5", "model": "composer-2.5-fast" }
        ]});
        let weekly = weekly_usage(&events).unwrap();
        assert_eq!(weekly.last().unwrap().requests, 1.5);
        let models = models_from_events(&events);
        assert_eq!(models[0].name, "composer-2.5-fast");
        assert_eq!(models[0].requests, 2);
    }

    #[test]
    fn weekly_usage_returns_fixed_seven_day_window() {
        let now_ms = OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
        let events = serde_json::json!({ "usageEventsDisplay": [
            { "timestamp": now_ms.to_string(), "requestsCosts": 2.5, "chargedCents": 10.0, "kind": "USAGE_EVENT_KIND_USAGE_BASED" }
        ]});
        let weekly = weekly_usage(&events).unwrap();
        assert_eq!(weekly.len(), 7);
        assert_eq!(weekly.last().unwrap().requests, 2.5);
        assert!(weekly.last().unwrap().is_on_demand);
        assert_eq!(weekly.last().unwrap().on_demand_cents, 10.0);
    }

    #[test]
    fn usage_pools_split_team_hard_limit_from_api_plan() {
        let summary = serde_json::json!({
            "individualUsage": {
                "plan": {
                    "used": 2000,
                    "limit": 2000,
                    "totalPercentUsed": 38.266666666666666,
                    "breakdown": { "included": 2000, "bonus": 4888, "total": 6888 }
                },
                "onDemand": { "used": 0, "limit": null }
            }
        });
        let hard_limit = serde_json::json!({ "perUserMonthlyLimitDollars": 180 });
        let (primary, on_demand) = usage_pools(&summary, Some(&hard_limit));
        assert_eq!(primary.kind, "currency");
        assert_eq!(primary.used, 6888.0);
        assert_eq!(primary.limit, Some(18000.0));
        assert!((primary.percent - 38.2666).abs() < 0.01);
        let on_demand = on_demand.expect("other models");
        assert_eq!(on_demand.used, 2000.0);
        assert_eq!(on_demand.limit, Some(2000.0));
        assert_eq!(on_demand.percent, 100.0);
    }

    #[test]
    fn usage_pools_infer_hard_limit_from_breakdown_percent() {
        let summary = serde_json::json!({
            "individualUsage": {
                "plan": {
                    "used": 2000,
                    "limit": 2000,
                    "totalPercentUsed": 32.04444444444445,
                    "breakdown": { "included": 2000, "bonus": 3768, "total": 5768 }
                }
            }
        });
        let (primary, on_demand) = usage_pools(&summary, None);
        assert_eq!(primary.used, 5768.0);
        assert_eq!(primary.limit, Some(18000.0));
        let on_demand = on_demand.expect("other models");
        assert_eq!(on_demand.used, 2000.0);
        assert_eq!(on_demand.limit, Some(2000.0));
    }

    #[test]
    fn usage_pools_do_not_echo_overage_as_limit_when_percent_is_capped() {
        let summary = serde_json::json!({
            "individualUsage": {
                "plan": {
                    "used": 2000,
                    "limit": 2000,
                    "totalPercentUsed": 100.0,
                    "autoPercentUsed": 100.0,
                    "apiPercentUsed": 100.0,
                    "breakdown": { "included": 2000, "bonus": 16172, "total": 18172 }
                }
            }
        });
        let (primary, on_demand) = usage_pools(&summary, None);
        assert_eq!(primary.used, 18172.0);
        assert_eq!(primary.limit, Some(18000.0));
        assert!((primary.percent - 100.955).abs() < 0.01);
        let on_demand = on_demand.expect("other models");
        assert_eq!(on_demand.used, 2000.0);
        assert_eq!(on_demand.limit, Some(2000.0));
    }

    #[test]
    fn usage_pools_pro_uses_auto_percent_and_plan_as_other_models() {
        let summary = serde_json::json!({
            "membershipType": "pro",
            "individualUsage": {
                "plan": {
                    "used": 1234,
                    "limit": 2000,
                    "autoPercentUsed": 12.5,
                    "apiPercentUsed": 61.7,
                    "totalPercentUsed": 40.0
                }
            }
        });
        let (primary, on_demand) = usage_pools(&summary, None);
        assert_eq!(primary.kind, "percent");
        assert_eq!(primary.percent, 12.5);
        let on_demand = on_demand.expect("other models");
        assert_eq!(on_demand.used, 1234.0);
        assert_eq!(on_demand.limit, Some(2000.0));
        assert_eq!(on_demand.percent, 61.7);
    }

    #[test]
    fn usage_pools_ultra_keeps_included_api_pool_as_other_models() {
        let summary = serde_json::json!({
            "membershipType": "ultra",
            "autoModelSelectedDisplayMessage": "You've used 2% of your included total usage",
            "namedModelSelectedDisplayMessage": "You've used 86% of your included API usage",
            "individualUsage": {
                "plan": {
                    "used": 34400,
                    "limit": 40000,
                    "autoPercentUsed": 2.0,
                    "apiPercentUsed": 86.0,
                    "totalPercentUsed": 86.0
                }
            }
        });
        let (primary, on_demand) = usage_pools(&summary, None);
        assert_eq!(primary.kind, "percent");
        assert_eq!(primary.percent, 2.0);
        let on_demand = on_demand.expect("other models");
        assert_eq!(on_demand.used, 34400.0);
        assert_eq!(on_demand.limit, Some(40000.0));
        assert_eq!(on_demand.percent, 86.0);
    }

    #[test]
    fn usage_pools_free_infers_a_currency_quota() {
        let summary = serde_json::json!({
            "membershipType": "free",
            "individualUsage": {
                "plan": {
                    "used": 0,
                    "limit": 0,
                    "autoPercentUsed": 100.0,
                    "apiPercentUsed": 0.0,
                    "totalPercentUsed": 61.5,
                    "breakdown": { "total": 123 }
                }
            }
        });
        let (primary, on_demand) = usage_pools(&summary, None);
        assert_eq!(primary.kind, "currency");
        assert_eq!(primary.used, 123.0);
        assert_eq!(primary.limit, Some(200.0));
        let on_demand = on_demand.expect("other models");
        assert_eq!(on_demand.kind, "percent");
        assert_eq!(on_demand.percent, 0.0);
    }

    #[test]
    fn usage_pools_keep_personal_plan_when_no_larger_cap() {
        let summary = serde_json::json!({
            "individualUsage": {
                "plan": { "used": 1200, "limit": 2000, "totalPercentUsed": 60.0, "breakdown": { "total": 1200 } },
                "onDemand": { "used": 400, "limit": 4000 }
            }
        });
        let (primary, on_demand) = usage_pools(&summary, None);
        assert_eq!(primary.used, 1200.0);
        assert_eq!(primary.limit, Some(2000.0));
        let on_demand = on_demand.expect("on demand");
        assert_eq!(on_demand.used, 400.0);
        assert_eq!(on_demand.limit, Some(4000.0));
    }

    #[test]
    fn usage_pools_token_enterprise_uses_overall_and_hard_limit() {
        let summary = serde_json::json!({
            "individualUsage": { "overall": { "used": 17, "limit": null } }
        });
        let hard_limit = serde_json::json!({ "perUserMonthlyLimitDollars": 100 });
        let (primary, on_demand) = usage_pools(&summary, Some(&hard_limit));
        assert_eq!(primary.used, 17.0);
        assert_eq!(primary.limit, Some(10000.0));
        assert!(on_demand.is_none());
    }

    #[test]
    fn models_from_events_sum_weighted_request_costs() {
        let events = serde_json::json!({ "usageEventsDisplay": [
            { "model": "composer-2.5-fast", "requestsCosts": 2.4 },
            { "modelName": "composer-2.5-fast", "requestsCosts": 1.6 },
            { "model": "gpt-5.5-medium", "requestsCosts": 8 },
            { "model": "ignored", "requestsCosts": 0 }
        ]});
        let models = models_from_events(&events);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "gpt-5.5-medium");
        assert_eq!(models[0].requests, 8);
        assert_eq!(models[1].name, "composer-2.5-fast");
        assert_eq!(models[1].requests, 4);
    }

    #[test]
    fn models_from_events_read_dashboard_model_name_field() {
        let events = serde_json::json!({ "usageEventsDisplay": [
            { "modelName": "cursor-model", "requestsCosts": 3.2 }
        ]});
        let models = models_from_events(&events);
        assert_eq!(models[0].name, "cursor-model");
        assert_eq!(models[0].requests, 3);
    }

    #[test]
    fn models_from_events_read_nested_model_object() {
        let events = serde_json::json!({ "usageEventsDisplay": [
            { "model": { "name": "composer-2.5-fast" }, "requestsCosts": 4 }
        ]});
        let models = models_from_events(&events);
        assert_eq!(models[0].name, "composer-2.5-fast");
        assert_eq!(models[0].requests, 4);
    }

    #[test]
    fn usage_event_rows_map_dashboard_fields_newest_first() {
        let events = serde_json::json!({ "usageEventsDisplay": [
            {
                "timestamp": 1_700_000_000_000u64,
                "model": "composer-2.5-fast",
                "requestsCosts": 1.5,
                "inputTokens": 12,
                "outputTokens": 4,
                "costUsd": 0.2
            },
            {
                "timestamp": "1700000001000",
                "modelName": "gpt-5.5-medium",
                "requestsCosts": 3,
                "chargedCents": 25,
                "kind": "USAGE_EVENT_KIND_USAGE_BASED"
            }
        ]});
        let rows = usage_event_rows(&events);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].timestamp, 1_700_000_001_000);
        assert_eq!(rows[0].model.as_deref(), Some("gpt-5.5-medium"));
        assert!(rows[0].on_demand);
        assert_eq!(rows[0].charged_cents, Some(25.0));
        assert_eq!(rows[1].model.as_deref(), Some("composer-2.5-fast"));
        assert_eq!(rows[1].input_tokens, Some(12.0));
        assert_eq!(rows[1].output_tokens, Some(4.0));
        assert!(!rows[1].on_demand);
    }

    #[test]
    fn usage_events_body_omits_null_user_id() {
        let personal = usage_events_body(Some(0), None, 1, false);
        assert_eq!(personal["teamId"], 0);
        assert_eq!(personal["page"], 1);
        assert_eq!(personal["pageSize"], 100);
        assert!(personal.get("userId").is_none());
        assert!(personal.get("startDate").is_none());
        let enterprise = usage_events_body(Some(77), Some(42), 2, true);
        assert_eq!(enterprise["teamId"], 77);
        assert_eq!(enterprise["userId"], 42);
        assert_eq!(enterprise["page"], 2);
        assert!(enterprise
            .get("startDate")
            .and_then(serde_json::Value::as_str)
            .is_some());
        assert!(enterprise
            .get("endDate")
            .and_then(serde_json::Value::as_str)
            .is_some());
        let unscoped = usage_events_body(None, Some(9), 1, false);
        assert!(unscoped.get("teamId").is_none());
        assert_eq!(unscoped["userId"], 9);
    }

    #[test]
    fn usage_events_from_response_treats_missing_or_null_as_empty() {
        assert_eq!(
            usage_events_from_response(&serde_json::json!({ "totalUsageEventsCount": 0 })),
            Some(vec![])
        );
        assert_eq!(
            usage_events_from_response(&serde_json::json!({ "usageEventsDisplay": null })),
            Some(vec![])
        );
        assert_eq!(
            usage_events_from_response(
                &serde_json::json!({ "usageEvents": [{ "model": "composer-2.5-fast" }] })
            )
            .unwrap()
            .len(),
            1
        );
    }

    #[test]
    fn auth_numeric_id_reads_dashboard_me_id() {
        assert_eq!(
            auth_numeric_id(&serde_json::json!({ "id": "232352588", "email": "a@b.c" })),
            Some(232_352_588)
        );
        assert_eq!(
            auth_numeric_id(&serde_json::json!({ "userId": 42 })),
            Some(42)
        );
    }

    #[test]
    fn first_team_id_reads_string_and_numeric_ids() {
        assert_eq!(
            first_team_id(&serde_json::json!({ "teams": [{ "id": "13403082", "name": "acme" }] })),
            Some(13_403_082)
        );
        assert_eq!(
            first_team_id(&serde_json::json!({ "teams": [{ "teamId": 9 }] })),
            Some(9)
        );
    }

    #[test]
    fn team_member_user_id_trims_email_and_accepts_string_ids() {
        let spend = serde_json::json!({
            "teamMemberSpend": [
                { "userId": "232352588", "email": " User@Example.com " },
                { "id": 1, "email": "other@example.com" }
            ]
        });
        assert_eq!(
            team_member_user_id(&spend, &["user@example.com".into()]),
            Some(232_352_588)
        );
        assert_eq!(
            team_member_user_id(&spend, &[" missing@example.com ".into()]),
            None
        );
    }

    #[test]
    fn usage_snapshot_is_serializable_without_credentials() {
        let snapshot = CursorUsageDetails {
            account_id: "acc_test".into(),
            label: "Test".into(),
            email: Some("me@example.com".into()),
            name: None,
            membership_type: Some("pro".into()),
            primary: usage_metric("currency", 25.0, Some(100.0)),
            reset_at: None,
            on_demand: None,
            models: vec![],
            weekly: vec![],
            weekly_available: false,
            weekly_error: None,
            events: vec![],
            checked_at: 1,
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("checkedAt"));
        assert!(!json.contains("accessToken"));
        assert!(!json.contains("WorkosCursorSessionToken"));
    }
    #[test]
    fn usage_export_matches_the_requested_shape_without_internal_sources() {
        let mut record = serde_json::json!({ "id": "cursor_source" });
        update_export_usage(
            &mut record,
            serde_json::json!({
                "auth_me": { "email": "me@example.com" },
                "usage_summary": { "membershipType": "enterprise", "billingCycleEnd": "2026-08-27T00:00:00.000Z" },
                "usage": { "fallback": { "numRequests": 2 } },
                "usage_events": { "usageEventsDisplay": [{
                    "model": "cursor-model", "requestsCosts": 1, "inputTokens": 12, "outputTokens": 5, "costUsd": 1.25
                }, {
                    "model": "cursor-model", "requestsCosts": 1, "inputTokens": 3, "outputTokens": 7, "costUsd": 0.75
                }] }
            }),
            42,
        );
        assert_eq!(record["cursor_usage_raw"]["membershipType"], "enterprise");
        assert_eq!(record["cursor_usage_raw"]["total_input_tokens"], 15.0);
        assert_eq!(record["cursor_usage_raw"]["total_output_tokens"], 12.0);
        assert_eq!(
            record["cursor_usage_raw"]["used_models"][0]["model_name"],
            "cursor-model"
        );
        assert_eq!(
            record["cursor_usage_raw"]["used_models"][0]["num_requests"],
            2
        );
        assert_eq!(
            record["cursor_usage_raw"]["used_models"][0]["cost_usd"],
            2.0
        );
        assert!(record.get("cursor_usage_sources").is_none());
        assert!(record.get("total_input_tokens").is_none());
    }
}
