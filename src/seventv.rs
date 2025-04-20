use futures::AsyncReadExt as _;
use futures::future;
use gpui::http_client::AsyncBody;
use gpui::http_client::HttpClient;
use reqwest_client::ReqwestClient;
use serde::{Deserialize, Serialize};
use serde_json;
use sha256::digest;
use std::env;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

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

async fn query_7tv(query: String) -> Vec<Item> {
    let payload = Payload {
        query: GQL_QUERY,
        // @NOTE: there are also optional filters and other options.
        variables: Variables {
            query,
            tags: vec![],
            sort_by: "TOP_ALL_TIME".to_string(), // @TODO: should be an enum
            page: 1,
            per_page: 15,
        },
    };

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
    return serde_json::from_str::<Data>(&raw_response)
        .expect("rip json response load")
        .data
        .emotes
        .search
        .items;
}

#[derive(Debug)]
pub struct WebmEmote {
    pub id: String,
    pub name: String,
    pub path: String,
}

async fn download_webm_if_not_cached(emotes: &Vec<Item>) -> Vec<WebmEmote> {
    let webm_dir = format!("{}/.cache/emotespicker/webm", env::var("HOME").unwrap());
    fs::create_dir_all(webm_dir.clone()).expect("rip webm dir");
    // let mut webms = vec![];
    let client = ReqwestClient::new();

    let results = future::join_all(emotes.into_iter().map(|emote| {
        let client = &client;
        let webm_dir = webm_dir.clone();

        async move {
            let webm_fp = format!("{}/{}.webp", webm_dir.clone(), emote.id.clone());
            // We are assuming here that if failed to create - it already exists..
            let gif_entry = emote.images.iter().find(|o| {
                // println!("ENTRY: {:?}", o);
                o.scale == 4 && o.mime == "image/webp"
            });

            if let Some(entry) = gif_entry {
                if Path::new(&webm_fp).exists() {
                    println!("Using cached file: {:?}!", webm_fp.clone());
                } else {
                    if let Ok(mut file) = File::create(webm_fp.clone()) {
                        println!("DOWNLOADING: {:?}", entry.url.clone());

                        let mut raw_response = Vec::new();
                        client
                            .get(&entry.url, AsyncBody::empty(), true)
                            .await
                            .expect("rip download request")
                            .into_body()
                            .read_to_end(&mut raw_response)
                            .await
                            .expect(&format!("rip download body: {}", &entry.url));
                        file.write_all(&raw_response).expect("rip write file");
                    }
                }
            } else {
                println!("NO GIF FORMAT FOR EMOTE {:?}", emote.name.clone());
                return None;
            }

            Some(WebmEmote {
                id: emote.id.clone(),
                name: emote.name.clone(),
                path: webm_fp.clone(),
            })
        }
    }))
    .await;

    return results.into_iter().filter_map(|e| e).collect();
}

pub async fn get_7tv(query: String) -> Vec<WebmEmote> {
    let queries_dir = format!("{}/.cache/emotespicker/queries", env::var("HOME").expect("rip home"));
    fs::create_dir_all(queries_dir.clone()).expect("rip queries dir");
    let query_fp = format!("{}/{}.json", queries_dir, digest(query.clone()));
    if let Ok(mut file) = File::open(query_fp.clone()) {
        let mut contents = String::new();
        file.read_to_string(&mut contents).expect("rip file read");
        let emotes: Vec<Item> = serde_json::from_str(&contents).expect("rip json load");
        download_webm_if_not_cached(&emotes).await
    } else {
        println!("QUERYING: {:?}", query.clone());
        let emotes = query_7tv(query).await;
        let mut file = File::create(query_fp.clone()).expect("rip create file");
        file.write_all(serde_json::to_vec_pretty(&emotes).unwrap().as_ref())
            .expect("rip write file");
        download_webm_if_not_cached(&emotes).await
    }
}
