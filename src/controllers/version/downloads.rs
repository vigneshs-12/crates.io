//! Functionality for downloading crates and maintaining download counts
//!
//! Crate level functionality is located in `krate::downloads`.

use super::version_and_crate;
use crate::controllers::prelude::*;
use crate::models::VersionDownload;
use crate::schema::*;
use crate::util::errors::version_not_found;
use crate::views::EncodableVersionDownload;
use chrono::{Duration, NaiveDate, Utc};

/// Handles the `GET /crates/:crate_id/:version/download` route.
/// This returns a URL to the location where the crate is stored.
pub async fn download(
    app: AppState,
    Path((crate_name, version)): Path<(String, String)>,
    req: Parts,
) -> AppResult<Response> {
    let wants_json = req.wants_json();
    let redirect_url = app.storage.crate_location(&crate_name, &version);
    if wants_json {
        Ok(Json(json!({ "url": redirect_url })).into_response())
    } else {
        Ok(redirect(redirect_url))
    }
}

#[instrument("db.query", skip(conn), fields(message = "SELECT ... FROM versions"))]
fn get_version_id(krate: &str, version: &str, conn: &mut PgConnection) -> QueryResult<i32> {
    versions::table
        .inner_join(crates::table)
        .select(versions::id)
        .filter(crates::name.eq(&krate))
        .filter(versions::num.eq(&version))
        .first::<i32>(conn)
}

/// Handles the `GET /crates/:crate_id/:version/downloads` route.
pub async fn downloads(
    app: AppState,
    Path((crate_name, version)): Path<(String, String)>,
    req: Parts,
) -> AppResult<Json<Value>> {
    spawn_blocking(move || {
        if semver::Version::parse(&version).is_err() {
            return Err(version_not_found(&crate_name, &version));
        }

        let conn = &mut *app.db_read()?;
        let (version, _) = version_and_crate(conn, &crate_name, &version)?;

        let cutoff_end_date = req
            .query()
            .get("before_date")
            .and_then(|d| NaiveDate::parse_from_str(d, "%F").ok())
            .unwrap_or_else(|| Utc::now().date_naive());
        let cutoff_start_date = cutoff_end_date - Duration::days(89);

        let downloads = VersionDownload::belonging_to(&version)
            .filter(version_downloads::date.between(cutoff_start_date, cutoff_end_date))
            .order(version_downloads::date)
            .load(conn)?
            .into_iter()
            .map(VersionDownload::into)
            .collect::<Vec<EncodableVersionDownload>>();

        Ok(Json(json!({ "version_downloads": downloads })))
    })
    .await
}
