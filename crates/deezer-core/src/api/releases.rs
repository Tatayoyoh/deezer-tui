use std::collections::HashSet;

use serde::Deserialize;
use serde_json::Value;

use super::models::{AlbumData, DeezerError};

const NEW_RELEASES_URL: &str = "https://www.deezer.com/en/channels/new";

/// Fetch the albums in Deezer's New Releases carousel.
pub async fn get_new_releases(http: &reqwest::Client) -> Result<Vec<AlbumData>, DeezerError> {
    let page = fetch_page(http, NEW_RELEASES_URL).await?;

    parse_new_releases(&page)
}

/// Fetch the albums in the new releases row of a genre's channel page.
///
/// `genre_id` is a public API genre id. Genres whose channel has no such row
/// yield an empty list.
pub async fn get_genre_releases(
    http: &reqwest::Client,
    genre_id: u64,
) -> Result<Vec<AlbumData>, DeezerError> {
    let Some(slug) = genre_channel(genre_id) else {
        return Ok(Vec::new());
    };
    // The English page keeps the section titles matched below stable.
    let page = fetch_page(http, &format!("https://www.deezer.com/en/channels/{slug}")).await?;
    parse_genre_releases(&page)
}

fn genre_channel(genre_id: u64) -> Option<&'static str> {
    Some(match genre_id {
        2 => "afro",
        75 => "brazilian",
        84 => "country",
        85 => "alternative",
        98 => "classical",
        106 => "electronic",
        113 => "dance",
        116 => "rap",
        129 => "jazz",
        132 => "pop",
        144 => "reggae",
        152 => "rock",
        153 => "blues",
        165 => "rnb",
        197 => "latin",
        464 => "metal",
        466 => "folk",
        _ => return None,
    })
}

async fn fetch_page(http: &reqwest::Client, url: &str) -> Result<String, DeezerError> {
    http.get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| DeezerError::Http(e.to_string()))?
        .text()
        .await
        .map_err(|e| DeezerError::Http(e.to_string()))
}

fn parse_new_releases(page: &str) -> Result<Vec<AlbumData>, DeezerError> {
    // Use the release carousel's content source, independent of its translated title.
    parse_albums(page, |section| {
        section["section_id"].as_str().is_some_and(|id| {
            id.split(',').any(|field| {
                field == "content_source=album_content-source_unmissables-new-releases"
            })
        })
    })
}

fn parse_genre_releases(page: &str) -> Result<Vec<AlbumData>, DeezerError> {
    // Genre channels share content sources between their album rows, so only
    // the title ("Brand new releases", "New Pop releases") tells them apart.
    parse_albums(page, |section| {
        section["title"].as_str().is_some_and(|title| {
            let title = title.to_lowercase();
            title.ends_with(" releases") && title.split(' ').any(|word| word == "new")
        })
    })
}

fn parse_albums(
    page: &str,
    wanted: impl Fn(&Value) -> bool,
) -> Result<Vec<AlbumData>, DeezerError> {
    // The public editorial releases endpoint can return an empty feed. Deezer
    // embeds the current channel data as JSON in the web page.
    let (_, state) = page
        .split_once("window.__DZR_APP_STATE__ =")
        .ok_or_else(|| DeezerError::Api("Missing New Releases page data".into()))?;
    let state = Value::deserialize(&mut serde_json::Deserializer::from_str(state))
        .map_err(|e| DeezerError::Api(format!("Invalid New Releases page data: {e}")))?;
    let sections = state["sections"]
        .as_array()
        .ok_or_else(|| DeezerError::Api("Missing New Releases page sections".into()))?;

    let mut albums: Vec<_> = sections
        .iter()
        .filter(|section| wanted(section))
        .filter_map(|section| section["items"].as_array())
        .flatten()
        .filter_map(parse_album)
        .collect();

    albums.sort_by(|a, b| b.release_date.cmp(&a.release_date));
    let mut seen = HashSet::new();
    albums.retain(|album| seen.insert(album.album_id.clone()));
    Ok(albums)
}

fn parse_album(item: &Value) -> Option<AlbumData> {
    let data = &item["data"];
    if item["type"] != "album" || data["AVAILABLE"].as_bool() == Some(false) {
        return None;
    }

    let album_id = match &item["id"] {
        Value::String(id) if !id.is_empty() => id.clone(),
        Value::Number(id) => id.as_u64()?.to_string(),
        _ => return None,
    };
    let release_date = [
        "ORIGINAL_RELEASE_DATE",
        "PHYSICAL_RELEASE_DATE",
        "DIGITAL_RELEASE_DATE",
    ]
    .into_iter()
    .filter_map(|key| data[key].as_str())
    .find(|date| !date.is_empty() && *date != "0000-00-00")
    .unwrap_or_default();

    Some(AlbumData {
        album_id,
        title: data["ALB_TITLE"].as_str()?.to_owned(),
        artist: data["ART_NAME"].as_str().unwrap_or_default().to_owned(),
        nb_tracks: 0,
        release_date: release_date.to_owned(),
        nb_fan: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_recent_albums_from_the_release_carousel_in_date_order() {
        let older = json!({
            "id": "10", "type": "album",
            "data": {"ALB_TITLE": "Older", "ART_NAME": "Artist A", "ORIGINAL_RELEASE_DATE": "2026-11-13"}
        });
        let newer = json!({
            "id": 20, "type": "album",
            "data": {"ALB_TITLE": "Newer", "ART_NAME": "Artist B", "DIGITAL_RELEASE_DATE": "2026-11-20"}
        });
        let state = json!({
            "sections": [
                {"section_id": "content_source=album_content-source_targeted-module",
                 "items": [{"id": "30", "type": "album", "data": {"ALB_TITLE": "Popular"}}]},
                {"section_id": "module_type=album,content_source=album_content-source_unmissables-new-releases,content_source_count=4",
                 "items": [
                     older, newer, newer,
                     {"id": "40", "type": "playlist", "data": {"ALB_TITLE": "Playlist"}},
                     {"id": "50", "type": "album", "data": {"ALB_TITLE": "Unavailable", "AVAILABLE": false}},
                     {"type": "album", "data": {"ALB_TITLE": "Missing ID"}}
                 ]}
            ]
        });
        let page = format!("<script>window.__DZR_APP_STATE__ = {state};</script>");
        let albums = parse_new_releases(&page).unwrap();

        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].album_id, "20");
        assert_eq!(albums[0].title, "Newer");
        assert_eq!(albums[0].artist, "Artist B");
        assert_eq!(albums[0].release_date, "2026-11-20");
        assert_eq!(albums[1].album_id, "10");
    }

    #[test]
    fn picks_only_the_new_releases_row_of_a_genre_channel() {
        let row = |title: &str, id: &str| {
            json!({"title": title, "items": [
                {"id": id, "type": "album", "data": {"ALB_TITLE": title}}
            ]})
        };
        let state = json!({"sections": [
            row("Brand new releases", "1"),
            row("New Rock releases", "2"),
            row("Albums we have on repeat", "3"),
            row("Rock reissues", "4"),
        ]});
        let page = format!("<script>window.__DZR_APP_STATE__ = {state};</script>");
        let ids: Vec<_> = parse_genre_releases(&page)
            .unwrap()
            .into_iter()
            .map(|album| album.album_id)
            .collect();

        assert_eq!(ids, ["1", "2"]);
    }

    #[test]
    fn distinguishes_an_empty_feed_from_an_invalid_page() {
        let empty = r#"<script>window.__DZR_APP_STATE__ = {"sections":[]};</script>"#;
        assert!(parse_new_releases(empty).unwrap().is_empty());
        assert!(parse_new_releases("<html>Missing page state</html>").is_err());
    }
}
