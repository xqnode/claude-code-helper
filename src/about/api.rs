use axum::response::Html;

const ABOUT_HTML: &str = include_str!("page.html");
const BRAND_ICON_SVG: &str = include_str!("../../assets/brand-icon.svg");

pub async fn about_page() -> Html<String> {
    Html(
        ABOUT_HTML
            .replace("<!--BRAND_ICON-->", BRAND_ICON_SVG.trim())
            .replace("<!--VERSION-->", env!("CARGO_PKG_VERSION")),
    )
}
