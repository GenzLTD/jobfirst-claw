//! Portal contract adapter.
//!
//! Public, read-only surface consumed by the company portal SPA
//! (`szbolent-portal`). It exists so the portal's Page Engine contract
//! (`GET /v1/menus?product=…`, `GET /v1/pages/{slug}`) is served from the same
//! memory store the agent writes to, instead of a static menu table.
//!
//! Routes here are intentionally *public* (no bearer token): every visitor's
//! browser fetches the menu tree before any authentication happens, so only
//! content that is safe to publish anonymously may live under these keys.
//!
//! Memory keys owned by this module:
//!
//! | key                     | value                                              |
//! |-------------------------|----------------------------------------------------|
//! | `portal.menus.<product>`| JSON array of [`PortalMenuItem`]                    |
//! | `portal.pages.<slug>`   | JSON page schema consumed by `usePageRenderer`      |
//!
//! When a menu key is missing the adapter seeds a built-in baseline for the
//! `szbolent` product so a fresh install still renders navigation, then stores
//! it. From then on the stored copy is the source of truth: an operator — or an
//! agent writing through `/api/memory` — edits that entry and the portal
//! follows on the next page load. That is what makes this layer agent-owned
//! rather than a hand-maintained table.

use super::AppState;
use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
};
use serde::{Deserialize, Serialize};
use zeroclaw_memory::MemoryCategory;

/// Memory key prefix holding the per-product menu tree.
const MENU_KEY_PREFIX: &str = "portal.menus.";
/// Memory key prefix holding authored page schemas.
const PAGE_KEY_PREFIX: &str = "portal.pages.";
/// Memory category stamped on every entry this adapter writes.
const PORTAL_MEMORY_CATEGORY: &str = "portal";

/// `GET /v1/menus` query parameters.
#[derive(Debug, Deserialize)]
pub struct MenusQuery {
    pub product: Option<String>,
}

/// One node of the portal navigation tree.
///
/// Field names mirror the `MenuItem` TypeScript interface in
/// `szbolent-portal/src/api/page-engine.types.ts` — camelCase on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalMenuItem {
    pub id: u32,
    #[serde(default)]
    pub parent_id: Option<u32>,
    pub title: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(default = "default_visibility")]
    pub visible_to: String,
    #[serde(default)]
    pub sort_order: i32,
    pub product: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<PortalMenuItem>>,
}

fn default_visibility() -> String {
    "public".to_string()
}

/// Build one leaf node. `component` is a `src/views/<name>.vue` stem; the
/// portal resolves it through its `import.meta.glob` view registry.
fn leaf(
    id: u32,
    title: &str,
    path: &str,
    icon: &str,
    component: &str,
    sort_order: i32,
    product: &str,
) -> PortalMenuItem {
    PortalMenuItem {
        id,
        parent_id: None,
        title: title.to_string(),
        path: path.to_string(),
        icon: Some(icon.to_string()),
        component: Some(component.to_string()),
        visible_to: default_visibility(),
        sort_order,
        product: product.to_string(),
        children: None,
    }
}

/// Built-in baseline navigation, used only until an operator or agent writes
/// `portal.menus.<product>`.
///
/// Every `path` maps to a view that already ships in the portal, so the seeded
/// tree is immediately routable. The portal skips injecting a dynamic route
/// when a static route already claims the same path, which keeps this additive.
fn baseline_menus(product: &str) -> Vec<PortalMenuItem> {
    if product != "szbolent" {
        return Vec::new();
    }
    vec![
        leaf(1, "首页", "/", "home", "Home", 0, product),
        leaf(2, "服务", "/services", "grid", "Services", 1, product),
        leaf(3, "案例", "/case-study", "folder", "CaseStudy", 2, product),
        leaf(4, "关于", "/about", "info", "About", 3, product),
        leaf(5, "博客", "/blog", "edit", "Blog", 4, product),
        leaf(6, "招聘", "/careers", "briefcase", "Careers", 5, product),
        leaf(7, "联系", "/contact", "mail", "Contact", 6, product),
    ]
}

/// Menus for one product, seeding the baseline on first read.
async fn menus_for(state: &AppState, product: &str) -> Result<Vec<PortalMenuItem>, String> {
    let key = format!("{MENU_KEY_PREFIX}{product}");

    let stored = state
        .mem
        .get(&key)
        .await
        .map_err(|e| format!("memory read failed: {e}"))?;

    if let Some(entry) = stored {
        return serde_json::from_str::<Vec<PortalMenuItem>>(&entry.content)
            .map_err(|e| format!("stored menu tree is not valid JSON: {e}"));
    }

    let seeded = baseline_menus(product);
    if !seeded.is_empty() {
        let body = serde_json::to_string(&seeded).map_err(|e| format!("encode failed: {e}"))?;
        if let Err(e) = state
            .mem
            .store(
                &key,
                &body,
                MemoryCategory::Custom(PORTAL_MEMORY_CATEGORY.to_string()),
                None,
            )
            .await
        {
            // A read-only backend must not break navigation; log and serve the
            // freshly built baseline for this request.
            ::zeroclaw_log::record!(
                WARN,
                ::zeroclaw_log::Event::new(module_path!(), ::zeroclaw_log::Action::Note)
                    .with_outcome(::zeroclaw_log::EventOutcome::Unknown)
                    .with_attrs(::serde_json::json!({"key": key, "error": format!("{e}")})),
                "portal menu baseline could not be persisted; serving it for this request only"
            );
        }
    }
    Ok(seeded)
}

/// `GET /v1/menus?product=szbolent` — the portal navigation tree.
///
/// Returns a bare JSON array (no envelope), matching the portal's `apiGet`.
pub async fn handle_menus(
    State(state): State<AppState>,
    Query(query): Query<MenusQuery>,
) -> impl IntoResponse {
    let product = query.product.unwrap_or_else(|| "szbolent".to_string());

    match menus_for(&state, &product).await {
        Ok(items) => Json(items).into_response(),
        Err(message) => {
            ::zeroclaw_log::record!(
                ERROR,
                ::zeroclaw_log::Event::new(module_path!(), ::zeroclaw_log::Action::Note)
                    .with_outcome(::zeroclaw_log::EventOutcome::Failure)
                    .with_attrs(::serde_json::json!({"product": product, "error": message})),
                "portal menu lookup failed"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "menu_unavailable", "message": message })),
            )
                .into_response()
        }
    }
}

/// `GET /v1/pages/{slug}` — an authored page schema, if one has been published.
///
/// Returns 404 when the slug has no stored schema so the portal can fall back to
/// its static view of that route.
pub async fn handle_page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let key = format!("{PAGE_KEY_PREFIX}{slug}");

    match state.mem.get(&key).await {
        Ok(Some(entry)) => match serde_json::from_str::<serde_json::Value>(&entry.content) {
            Ok(page) => Json(page).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "page_parse_failed",
                    "message": format!("{e}"),
                })),
            )
                .into_response(),
        },
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "page_not_found",
                "message": format!("No authored page schema stored at {key}"),
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "page_unavailable",
                "message": format!("{e}"),
            })),
        )
            .into_response(),
    }
}

/// Public portal routes. Mounted outside the `/api/*` bearer boundary.
pub fn portal_routes() -> Router<AppState> {
    Router::new()
        .route("/v1/menus", get(handle_menus))
        .route("/v1/pages/{slug}", get(handle_page))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_menu_paths_are_unique_and_ordered() {
        let menus = baseline_menus("szbolent");
        let mut paths: Vec<&str> = menus.iter().map(|m| m.path.as_str()).collect();
        let count = paths.len();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), count, "duplicate paths in the baseline tree");

        let mut orders: Vec<i32> = menus.iter().map(|m| m.sort_order).collect();
        let sorted = {
            let mut o = orders.clone();
            o.sort_unstable();
            o
        };
        orders.dedup();
        assert_eq!(orders, sorted, "sort_order must be distinct and ascending");
    }

    #[test]
    fn baseline_menu_serializes_camel_case_without_nulls() {
        let one = vec![leaf(1, "首页", "/", "home", "Home", 0, "szbolent")];
        let json = serde_json::to_value(&one).expect("serialize");
        let node = &json[0];
        assert!(node.get("parentId").is_some(), "parentId must be camelCase");
        assert!(
            node.get("sortOrder").is_some(),
            "sortOrder must be camelCase"
        );
        assert!(
            node.get("visibleTo").is_some(),
            "visibleTo must be camelCase"
        );
        assert!(
            node.get("children").is_none(),
            "absent children must be omitted, not null"
        );
        assert_eq!(node["visibleTo"], "public");
        assert_eq!(node["component"], "Home");
    }

    #[test]
    fn unknown_product_has_no_baseline() {
        assert!(baseline_menus("some-other-product").is_empty());
    }
}
