//! Implementation coordination — `xsil request` (Phase C / PR-C10).
//!
//! Off-platform funding only (`fundingContactEmail` + `fundingNote`). Rejects
//! structured payment fields before any registry call.

use anyhow::{bail, Context, Result};
use colored::*;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;

use crate::registry::RegistryClient;
use crate::types::ImplementationRequest;

const TITLE_MIN: usize = 5;
const TITLE_MAX: usize = 200;
const DESCRIPTION_MIN: usize = 50;
const DESCRIPTION_MAX: usize = 10_000;
const TARGET_CAPABILITY_MAX: usize = 120;
const ACCEPTANCE_CRITERIA_MAX: usize = 4000;
const FUNDING_NOTE_MAX: usize = 2000;
const INTEREST_MESSAGE_MAX: usize = 500;

const FORBIDDEN_MONEY_KEYS: &[&str] = &[
    "amount", "currency", "wallet", "txhash", "payout", "escrow", "usdc", "funded", "paid",
];

/// Reject bodies that carry on-platform payment fields (mirrors registry guard).
pub fn reject_structured_money(value: &Value) -> Option<String> {
    let mut keys = Vec::new();
    collect_object_keys(value, &mut keys);
    for key in keys {
        let lower = key.to_ascii_lowercase();
        if FORBIDDEN_MONEY_KEYS.contains(&lower.as_str()) {
            return Some(format!(
                "Field \"{key}\" is not allowed on implementation requests (no on-platform payments)."
            ));
        }
    }
    None
}

fn collect_object_keys(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_object_keys(item, out);
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                out.push(k.clone());
                collect_object_keys(v, out);
            }
        }
        _ => {}
    }
}

fn optional_trimmed(s: Option<&str>, max: usize, field: &str) -> Result<Option<String>> {
    let Some(raw) = s else {
        return Ok(None);
    };
    let t = raw.trim();
    if t.is_empty() {
        return Ok(None);
    }
    if t.chars().count() > max {
        bail!("{field} must be at most {max} characters.");
    }
    Ok(Some(t.to_string()))
}

fn validate_title(title: &str) -> Result<()> {
    let t = title.trim();
    let n = t.chars().count();
    if !(TITLE_MIN..=TITLE_MAX).contains(&n) {
        bail!("title must be between {TITLE_MIN} and {TITLE_MAX} characters.");
    }
    Ok(())
}

fn validate_description(description: &str) -> Result<()> {
    let t = description.trim();
    let n = t.chars().count();
    if !(DESCRIPTION_MIN..=DESCRIPTION_MAX).contains(&n) {
        bail!("description must be between {DESCRIPTION_MIN} and {DESCRIPTION_MAX} characters.");
    }
    Ok(())
}

fn parse_visibility(raw: &str) -> Result<&'static str> {
    match raw.trim() {
        "public" => Ok("public"),
        "org_only" => Ok("org_only"),
        "unlisted" => Ok("unlisted"),
        other => bail!("visibility must be public, org_only, or unlisted (got \"{other}\")"),
    }
}

fn send_body(_registry: &RegistryClient, body: &Value) -> Result<()> {
    if let Some(msg) = reject_structured_money(body) {
        bail!("{msg}");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn cmd_create(
    registry: &RegistryClient,
    package: &str,
    title: &str,
    description: &str,
    description_file: Option<&Path>,
    visibility: &str,
    target_capability: Option<&str>,
    acceptance_criteria: Option<&str>,
    acceptance_file: Option<&Path>,
    funding_email: Option<&str>,
    funding_note: Option<&str>,
    org_id: Option<u32>,
    dry_run: bool,
) -> Result<()> {
    validate_title(title)?;
    let description = if let Some(path) = description_file {
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?
    } else if description.trim().is_empty() {
        bail!("Provide --description or --description-file.");
    } else {
        description.to_string()
    };
    validate_description(&description)?;

    let vis = parse_visibility(visibility)?;
    let target_capability =
        optional_trimmed(target_capability, TARGET_CAPABILITY_MAX, "targetCapability")?;
    let acceptance_criteria = if let Some(path) = acceptance_file {
        let text = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        optional_trimmed(
            Some(&text),
            ACCEPTANCE_CRITERIA_MAX,
            "acceptanceCriteriaSummary",
        )?
    } else {
        optional_trimmed(
            acceptance_criteria,
            ACCEPTANCE_CRITERIA_MAX,
            "acceptanceCriteriaSummary",
        )?
    };
    let funding_note = optional_trimmed(funding_note, FUNDING_NOTE_MAX, "fundingNote")?;
    let funding_email = funding_email
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let mut body = Map::new();
    body.insert("title".into(), json!(title.trim()));
    body.insert("description".into(), json!(description.trim()));
    body.insert("visibility".into(), json!(vis));
    if let Some(v) = target_capability {
        body.insert("targetCapability".into(), json!(v));
    }
    if let Some(v) = acceptance_criteria {
        body.insert("acceptanceCriteriaSummary".into(), json!(v));
    }
    if let Some(v) = funding_email {
        body.insert("fundingContactEmail".into(), json!(v));
    }
    if let Some(v) = funding_note {
        body.insert("fundingNote".into(), json!(v));
    }
    if let Some(id) = org_id {
        body.insert("createdByOrgId".into(), json!(id));
    }
    let body_val = Value::Object(body);
    send_body(registry, &body_val)?;

    if dry_run {
        println!(
            "{} Dry run: would create implementation request on {}",
            "✔".green(),
            package.bold()
        );
        return Ok(());
    }

    let req = registry.create_implementation_request(package, &body_val)?;
    print_request_summary(&req, true);
    println!(
        "  {} Run `xsil request open {}` to publish as open.",
        "→".dimmed(),
        req.id
    );
    Ok(())
}

pub fn cmd_list(
    registry: &RegistryClient,
    package: Option<&str>,
    status: Option<&str>,
    capability: Option<&str>,
) -> Result<()> {
    let requests = if let Some(slug) = package {
        registry.list_package_implementation_requests(slug)?
    } else {
        registry.list_implementation_requests(status, capability)?
    };

    if requests.is_empty() {
        println!("No implementation requests found.");
        return Ok(());
    }

    println!(
        "  {:<6} {:<10} {:<28} {:<36}",
        "id".dimmed(),
        "status".dimmed(),
        "package".dimmed(),
        "title".dimmed(),
    );
    for r in &requests {
        let slug = r.package.as_ref().map(|p| p.slug.as_str()).unwrap_or("?");
        println!(
            "  {:<6} {:<10} {:<28} {}",
            r.id,
            r.status,
            truncate(slug, 28),
            truncate(&r.title, 36),
        );
    }
    println!();
    println!(
        "  {} request(s). Use `xsil request show <id>` for details.",
        requests.len()
    );
    Ok(())
}

pub fn cmd_show(registry: &RegistryClient, id: u32) -> Result<()> {
    let req = registry.get_implementation_request(id)?;
    print_request_detail(&req);
    Ok(())
}

pub fn cmd_mine(registry: &RegistryClient) -> Result<()> {
    let requests = registry.list_my_implementation_requests()?;
    if requests.is_empty() {
        println!("You have no implementation requests (created, assigned, or interested).");
        return Ok(());
    }
    for r in &requests {
        print_request_summary(r, false);
        println!();
    }
    Ok(())
}

pub fn cmd_open(registry: &RegistryClient, id: u32, dry_run: bool) -> Result<()> {
    let body = json!({ "status": "open" });
    send_body(registry, &body)?;
    if dry_run {
        println!("{} Dry run: would open request #{}.", "✔".green(), id);
        return Ok(());
    }
    let req = registry.patch_implementation_request(id, &body)?;
    println!(
        "{} Request #{} is now {}.",
        "✔".green(),
        id,
        req.status.cyan()
    );
    Ok(())
}

pub fn cmd_cancel(registry: &RegistryClient, id: u32, dry_run: bool) -> Result<()> {
    let body = json!({ "status": "cancelled" });
    send_body(registry, &body)?;
    if dry_run {
        println!("{} Dry run: would cancel request #{}.", "✔".green(), id);
        return Ok(());
    }
    let req = registry.patch_implementation_request(id, &body)?;
    println!(
        "{} Request #{} is now {}.",
        "✔".green(),
        id,
        req.status.yellow()
    );
    Ok(())
}

pub fn cmd_interest(
    registry: &RegistryClient,
    id: u32,
    message: Option<&str>,
    dry_run: bool,
) -> Result<()> {
    let msg = optional_trimmed(message, INTEREST_MESSAGE_MAX, "message")?;
    let body = if let Some(m) = msg {
        json!({ "message": m })
    } else {
        json!({})
    };
    send_body(registry, &body)?;
    if dry_run {
        println!(
            "{} Dry run: would express interest in request #{}.",
            "✔".green(),
            id
        );
        return Ok(());
    }
    registry.create_implementation_interest(id, &body)?;
    println!("{} Interest recorded for request #{}.", "✔".green(), id);
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn print_request_summary(req: &ImplementationRequest, created: bool) {
    let slug = req.package.as_ref().map(|p| p.slug.as_str()).unwrap_or("?");
    let prefix = if created { "Created" } else { "Request" };
    println!(
        "{} {} #{} on {} — {}",
        "✔".green(),
        prefix,
        req.id,
        slug.bold(),
        req.title
    );
    println!("  status     : {}", req.status);
    if let Some(cap) = req.target_capability.as_deref().filter(|s| !s.is_empty()) {
        println!("  capability : {}", cap);
    }
    println!(
        "  interests  : {}   submissions : {}",
        req.interest_count, req.submission_count
    );
}

fn print_request_detail(req: &ImplementationRequest) {
    print_request_summary(req, false);
    println!("  visibility : {}", req.visibility);
    if let Some(by) = req.created_by.as_ref() {
        println!("  created by : {}", by.username);
    }
    if let Some(org) = req.created_by_org.as_ref() {
        println!("  org        : @{} ({})", org.slug, org.display_name);
    }
    if let Some(im) = req.assigned_implementer.as_ref() {
        println!("  assignee   : {}", im.username);
    }
    println!();
    println!("{}", req.description);
    if let Some(acc) = req
        .acceptance_criteria_summary
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        println!();
        println!("{}", "Acceptance criteria".bold());
        println!("{acc}");
    }
    if let Some(email) = req
        .funding_contact_email
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        println!();
        println!("{} Funding is off-platform. Contact: {}", "i".cyan(), email);
        if let Some(note) = req.funding_note.as_deref().filter(|s| !s.is_empty()) {
            println!("  {}", note.dimmed());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_structured_money_keys() {
        let body = json!({ "title": "x", "amount": 100 });
        assert!(reject_structured_money(&body).is_some());
    }

    #[test]
    fn allows_clean_body() {
        let body = json!({
            "title": "Add vector load",
            "description": "We need a community implementation of vector loads for this seeded package with tests and Spike integration.",
            "fundingContactEmail": "sponsor@example.com"
        });
        assert!(reject_structured_money(&body).is_none());
    }

    #[test]
    fn nested_money_key_rejected() {
        let body = json!({ "meta": { "wallet": "0xabc" } });
        assert!(reject_structured_money(&body).is_some());
    }

    #[test]
    fn visibility_parse() {
        assert!(parse_visibility("public").is_ok());
        assert!(parse_visibility("org_only").is_ok());
        assert!(parse_visibility("paid").is_err());
    }
}
