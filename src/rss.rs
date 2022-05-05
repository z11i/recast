use std::convert::Infallible;

use chrono::{DateTime, Duration, TimeZone};
use http::{Response, StatusCode};
use log::warn;
use rss::{Channel, Item};
use serde::{Deserialize, Serialize};
use urlencoding::decode;
use warp::{Rejection, Reply};

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct RawQuery {
    url: String,
    delay: String,
}

struct Query {
    url: String,
    delay: chrono::Duration,
}

impl Query {
    fn min_delay() -> chrono::Duration {
        Duration::hours(1)
    }
}

impl TryFrom<RawQuery> for Query {
    type Error = String;

    fn try_from(value: RawQuery) -> Result<Query, Self::Error> {
        let url = decode(&value.url)
            .map_err(|e| format!("failed to decode URL {}: {}", &value.url, e))?
            .into_owned();
        let delay = match value.delay.parse::<i64>() {
            Ok(d) => {
                let dh = chrono::Duration::hours(d);
                if dh < Query::min_delay() {
                    return Err(format!(
                        "delay must be at least {}",
                        Query::min_delay().num_hours()
                    ));
                } else {
                    dh
                }
            }
            Err(e) => {
                return Err(format!("delay must be an integer: {}", e));
            }
        };

        Ok(Query { url, delay })
    }
}

pub(crate) async fn handler(query: RawQuery) -> Result<impl Reply, Rejection> {
    let query: Query = query.try_into().map_err(|e: String| {
        warn!("failed to parse query: {}", e);
        warp::reject::custom(Error::QueryParse(e))
    })?;
    let url = query.url;
    let delay = query.delay;

    let res = reqwest::get(&url).await.map_err(|e| {
        warn!("failed to load feed: {}", e);
        warp::reject::custom(Error::FeedLoad(e.to_string()))
    })?;

    let h = res.headers().clone();

    let content = res.bytes().await.map_err(|e| {
        warn!("failed to read feed: {}", e);
        warp::reject::custom(Error::FeedLoad(e.to_string()))
    })?;

    let mut channel = Channel::read_from(&content[..]).map_err(|e| {
        warn!("failed to parse feed: {}", e);
        warp::reject::custom(Error::FeedParse(e.to_string()))
    })?;

    let new_items: Vec<Item> = channel
        .items_mut()
        .iter_mut()
        .filter_map(|i| postdate_item(i, delay))
        .collect();
    channel.set_items(new_items);

    let mut builder = Response::builder().status(StatusCode::OK);
    if let Some(ct) = h.get(http::header::CONTENT_TYPE) {
        builder = builder.header(http::header::CONTENT_TYPE, ct);
    }
    Ok(builder.body(channel.to_string()))
}

fn postdate_item(item: &mut Item, delay: Duration) -> Option<Item> {
    let orig_pubdate = item
        .pub_date()
        .and_then(|d| DateTime::parse_from_rfc2822(&d).ok())?;
    let new_pubdate = compare_time_after_delay(orig_pubdate, delay, chrono::Utc::now())?;
    item.set_pub_date(new_pubdate.to_rfc2822());

    if let Some(orig_desc) = item.description() {
        let new_desc = format!("(originally published on {}) {}", orig_pubdate, orig_desc);
        item.set_description(new_desc);
    }

    Some(item.to_owned())
}

fn compare_time_after_delay<T: TimeZone, U: TimeZone>(
    t: DateTime<T>,
    delay: Duration,
    now: DateTime<U>,
) -> Option<DateTime<T>> {
    t.checked_add_signed(delay)
        .and_then(|new_t| if new_t < now { Some(new_t) } else { None })
}

#[derive(Debug)]
enum Error {
    FeedLoad(String),
    FeedParse(String),
    QueryParse(String),
}

impl warp::reject::Reject for Error {}

pub(crate) async fn handle_error(err: Rejection) -> Result<impl Reply, Infallible> {
    let code;
    let message;

    if let Some(e) = err.find::<Error>() {
        (code, message) = match e {
            Error::FeedLoad(r) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to load feed: {}", r),
            ),
            Error::FeedParse(r) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to parse feed: {}", r),
            ),
            Error::QueryParse(r) => (
                StatusCode::BAD_REQUEST,
                format!("failed to parse query: {}", r),
            ),
        };
    } else {
        (code, message) = (
            StatusCode::INTERNAL_SERVER_ERROR,
            "unknown error".to_string(),
        );
    }

    Ok(warp::reply::with_status(message, code))
}

// #[cfg(test)]
// mod tests {
//     use crate::{
//         rss::{Query, RawQuery},
//         server,
//     };
//
//     #[async_std::test]
//     async fn try_from_raw_query() {
//         let query = RawQuery {
//             url: "https%3A%2F%2Fexample.com%2Frss.xml".to_string(),
//             delay: "1".to_string(),
//         };
//
//         let query: Query = query.try_into().unwrap();
//         assert_eq!(query.url, "https://example.com/rss.xml");
//         assert_eq!(query.delay, Query::min_delay());
//     }
//
//     #[async_std::test]
//     async fn handler_200() -> tide::Result<()> {
//         let url = "http://example.com/rss?url=https%3A%2F%2Fvideo%2Dapi%2Ewsj%2Ecom%2Fpodcast%2Frss%2Fwsj%2Ftech%2Dnews%2Dbriefing&delay=1";
//         let app = server();
//         let res = surf::Client::with_http_client(app).get(url).await?;
//         assert_eq!(res.status(), tide::StatusCode::Ok, "{:?}", res);
//         Ok(())
//     }
//
//     #[async_std::test]
//     async fn handler_400_invalid_query() -> tide::Result<()> {
//         let app = server();
//         let res = surf::Client::with_http_client(app)
//             .get("http://example.com/rss")
//             .await?;
//         assert_eq!(res.status(), tide::StatusCode::BadRequest);
//         Ok(())
//     }
// }
//
