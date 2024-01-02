use crate::builders::{CrateBuilder, VersionBuilder};
use crate::util::{MockAnonymousUser, RequestHelper, TestApp};
use chrono::{Duration, Utc};
use crates_io::views::EncodableVersionDownload;
use http::StatusCode;

#[derive(Deserialize)]
struct Downloads {
    version_downloads: Vec<EncodableVersionDownload>,
}

pub fn persist_downloads_count(app: &TestApp) {
    app.as_inner()
        .downloads_counter
        .persist_all_shards(app.as_inner())
        .expect("failed to persist downloads count")
        .log();
}

#[track_caller]
pub fn assert_dl_count(
    anon: &MockAnonymousUser,
    name_and_version: &str,
    query: Option<&str>,
    count: i32,
) {
    let url = format!("/api/v1/crates/{name_and_version}/downloads");
    let downloads: Downloads = if let Some(query) = query {
        anon.get_with_query(&url, query).good()
    } else {
        anon.get(&url).good()
    };
    let total_downloads = downloads
        .version_downloads
        .iter()
        .map(|vd| vd.downloads)
        .sum::<i32>();
    assert_eq!(total_downloads, count);
}

pub fn download(client: &impl RequestHelper, name_and_version: &str) {
    let url = format!("/api/v1/crates/{name_and_version}/download");
    let response = client.get::<()>(&url);
    assert_eq!(response.status(), StatusCode::FOUND);
}

#[test]
fn test_download() {
    let (app, anon, user) = TestApp::init().with_user();
    let user = user.as_model();

    app.db(|conn| {
        CrateBuilder::new("foo_download", user.id)
            .version(VersionBuilder::new("1.0.0"))
            .expect_build(conn);
    });

    // TODO: test the with_json code path
    download(&anon, "foo_download/1.0.0");
    // No downloads are counted until the counters are persisted
    assert_dl_count(&anon, "foo_download/1.0.0", None, 0);
    assert_dl_count(&anon, "foo_download", None, 0);
    persist_downloads_count(&app);
    // Now that the counters are persisted the download counts show up.
    assert_dl_count(&anon, "foo_download/1.0.0", None, 1);
    assert_dl_count(&anon, "foo_download", None, 1);

    let response = anon.get::<()>("/api/v1/crates/FOO_DOWNLOAD/1.0.0/download");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let yesterday = (Utc::now().date_naive() + Duration::days(-1)).format("%F");
    let query = format!("before_date={yesterday}");
    assert_dl_count(&anon, "foo_download/1.0.0", Some(&query), 0);
    // crate/downloads always returns the last 90 days and ignores date params
    assert_dl_count(&anon, "foo_download", Some(&query), 1);

    let tomorrow = (Utc::now().date_naive() + Duration::days(1)).format("%F");
    let query = format!("before_date={tomorrow}");
    assert_dl_count(&anon, "foo_download/1.0.0", Some(&query), 1);
    assert_dl_count(&anon, "foo_download", Some(&query), 1);
}
