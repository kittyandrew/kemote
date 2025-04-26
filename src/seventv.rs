use crate::CACHE_DIR;
use futures::AsyncReadExt as _;
use gpui::http_client::{AsyncBody, HttpClient};
use reqwest_client::ReqwestClient;
use serde::{Deserialize, Serialize};
use serde_json;

const GQL_QUERY: &str = r#"
query EmoteSearch(
    $query: String
    $tags: [String!]!
    $sortBy: SortBy!
    $filters: Filters
    $page: Int
    $perPage: Int!
    $isDefaultSetSet: Boolean!
    $defaultSetId: Id!
) {
    emotes {
        search(
            query: $query
            tags: { tags: $tags, match: ANY }
            sort: { sortBy: $sortBy, order: DESCENDING }
            filters: $filters
            page: $page
            perPage: $perPage
        ) {
            items {
                id
                defaultName
                owner {
                    mainConnection {
                        platformDisplayName
                    }
                    style {
                        activePaint {
                            id
                            name
                            data {
                                layers {
                                    id
                                    ty {
                                        __typename
                                        ... on PaintLayerTypeSingleColor {
                                            color {
                                                hex
                                            }
                                        }
                                        ... on PaintLayerTypeLinearGradient {
                                            angle
                                            repeating
                                            stops {
                                                at
                                                color {
                                                    hex
                                                }
                                            }
                                        }
                                        ... on PaintLayerTypeRadialGradient {
                                            repeating
                                            stops {
                                                at
                                                color {
                                                    hex
                                                }
                                            }
                                            shape
                                        }
                                        ... on PaintLayerTypeImage {
                                            images {
                                                url
                                                mime
                                                size
                                                scale
                                                width
                                                height
                                                frameCount
                                            }
                                        }
                                    }
                                    opacity
                                }
                                shadows {
                                    color {
                                        hex
                                    }
                                    offsetX
                                    offsetY
                                    blur
                                }
                            }
                        }
                    }
                    highestRoleColor {
                        hex
                    }
                }
                deleted
                flags {
                    # animated
                    # approvedPersonal
                    defaultZeroWidth
                    # deniedPersonal
                    # nsfw
                    private
                    publicListed
                }
                imagesPending
                images {
                    url
                    mime
                    size
                    scale
                    width
                    frameCount
                }
                ranking(ranking: TRENDING_WEEKLY)
                inEmoteSets(emoteSetIds: [$defaultSetId]) @include(if: $isDefaultSetSet) {
                    emoteSetId
                    emote {
                        id
                        alias
                    }
                }
            }
            totalCount
            pageCount
        }
    }
}
"#;

#[derive(Serialize)]
struct Variables {
    query: String,
    tags: Vec<String>,
    #[serde(rename(deserialize = "sortBy", serialize = "sortBy"))]
    sort_by: String,
    // filters: Option<Vec<String>>,
    page: usize,
    #[serde(rename(deserialize = "perPage", serialize = "perPage"))]
    per_page: usize,
    // isDefaultSet: boolean,
    // defaultSetId: String,
}

#[derive(Serialize)]
struct Payload<'a> {
    query: &'a str,
    variables: Variables,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Image {
    url: String,
    mime: String,
    size: usize,
    scale: usize,
    width: usize,
    #[serde(rename(deserialize = "frameCount", serialize = "frameCount"))]
    frame_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Item {
    id: String,
    #[serde(rename(deserialize = "defaultName", serialize = "defaultName"))]
    name: String,
    images: Vec<Image>,
}

#[derive(Debug, Deserialize)]
struct Items {
    items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
struct Search {
    search: Items,
}

#[derive(Debug, Deserialize)]
struct Emotes {
    emotes: Search,
}

#[derive(Debug, Deserialize)]
struct Data {
    data: Emotes,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WebmEmote {
    pub id: String,
    pub name: String,
    pub url: String,
}

impl WebmEmote {
    pub fn path(url: &String) -> String {
        format!("{}/webm/{}.webp", *CACHE_DIR, sha256::digest(url))
    }
}

pub async fn query_7tv(query: String) -> Vec<WebmEmote> {
    let payload = Payload {
        query: GQL_QUERY,
        // @NOTE: there are also optional filters and other options.
        variables: Variables {
            query,
            tags: vec![],
            sort_by: "TOP_ALL_TIME".to_string(), // @TODO: should be an enum
            page: 1,
            per_page: 50,
        },
    };

    // @TODO: Client can be created once for the whole gpui cx and passed in here?
    let client = ReqwestClient::new();
    let raw_payload = AsyncBody::from_bytes(serde_json::to_vec(&payload).expect("rip payload serialization").into());
    let mut raw_response = String::new(); // maybe replace with read to bytes
    client
        .post_json("https://api.7tv.app/v4/gql", raw_payload)
        .await
        .expect("rip gpui reqwest post json")
        .into_body()
        .read_to_string(&mut raw_response)
        .await
        .unwrap();

    // unpacking nested response schema
    let items = serde_json::from_str::<Data>(&raw_response)
        .expect("rip json response load")
        .data
        .emotes
        .search
        .items;

    items
        .into_iter()
        .map(|emote| {
            Some(WebmEmote {
                id: emote.id.clone(),
                name: emote.name.clone(),
                url: emote
                    .images
                    .iter()
                    .find(|o| o.scale == 4 && o.mime == "image/webp")
                    .expect("rip finding specified image mime")
                    .url
                    .clone(),
            })
        })
        .filter_map(|e| e)
        .collect()
}
