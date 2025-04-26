use crate::{CACHE_DIR, seventv::WebmEmote};
use futures::AsyncReadExt as _;
use futures::FutureExt;
use gpui::{
    App, AppContext, Asset, AssetLogger, Entity, ImageAssetLoader, ImageCache, ImageCacheError, ImageCacheItem,
    RenderImage, Resource, Window, hash, http_client::AsyncBody, http_client::HttpClient,
};
use reqwest_client::ReqwestClient;
use std::fs::{self, File};
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::{collections::HashMap, sync::Arc};

// Cache implementation, based on the default gpui cache, but with reads/writes to disk as an
// intermediate step between in-memory cache and loading from remote source.
pub struct HashMapImageCache {
    data: HashMap<u64, ImageCacheItem>,
    client: Arc<ReqwestClient>,
}

impl HashMapImageCache {
    /// Create a new image cache.
    #[inline]
    pub fn new(cx: &mut App) -> Entity<Self> {
        let e = cx.new(|_cx| HashMapImageCache {
            data: HashMap::new(),
            client: Arc::new(ReqwestClient::new()),
        });
        cx.observe_release(&e, |image_cache, cx| {
            for (_, mut item) in std::mem::replace(&mut image_cache.data, HashMap::new()) {
                if let Some(Ok(image)) = item.get() {
                    cx.drop_image(image, None);
                }
            }
        })
        .detach();
        e
    }

    /// Load an image from the given source.
    ///
    /// Returns `None` if the image is loading.
    pub fn load(
        &mut self,
        source: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let hash = hash(source);

        if let Some(item) = self.data.get_mut(&hash) {
            return item.get(); // will return None if still loading, I think - andrew
        }

        match source {
            Resource::Uri(uri) => {
                let source_url = format!("{}", uri);
                let new_source_path = WebmEmote::path(&source_url);
                let new_source = Resource::from(PathBuf::from(new_source_path.clone()));
                let fut = AssetLogger::<ImageAssetLoader>::load(new_source, cx);

                let client = self.client.clone();
                let task;
                if Path::new(&new_source_path).exists() {
                    task = cx.background_executor().spawn(fut).shared();
                } else {
                    task = cx
                        .background_executor()
                        .spawn(async move {
                            fs::create_dir_all(format!("{}/webm", *CACHE_DIR)).expect("rip webm dir");

                            let mut file = File::create(new_source_path).expect("rip webp file");
                            let mut raw_response = Vec::new();
                            client
                                .get(&source_url, AsyncBody::empty(), true)
                                .await
                                .expect("rip download request")
                                .into_body()
                                .read_to_end(&mut raw_response)
                                .await
                                .expect(&format!("rip download body: {}", &source_url));
                            file.write_all(&raw_response).expect("rip write file");

                            fut.await
                        })
                        .shared();
                }

                self.data.insert(hash, ImageCacheItem::Loading(task.clone()));

                let entity = window.current_view();
                window
                    .spawn(cx, {
                        async move |cx| {
                            _ = task.await;
                            cx.on_next_frame(move |_, cx| {
                                cx.notify(entity);
                            });
                        }
                    })
                    .detach();
            }
            // Those should be handled normally, since they are already in-memory
            _ => {
                let fut = AssetLogger::<ImageAssetLoader>::load(source.clone(), cx);
                let task = cx.background_executor().spawn(fut).shared();
                self.data.insert(hash, ImageCacheItem::Loading(task.clone()));

                let entity = window.current_view();
                window
                    .spawn(cx, {
                        async move |cx| {
                            _ = task.await;
                            cx.on_next_frame(move |_, cx| {
                                cx.notify(entity);
                            });
                        }
                    })
                    .detach();
            }
        }

        None
    }

    // @TODO: Should probably be unloading assets from memory at some point..
    //  - https://github.com/zed-industries/zed/blob/053fafa90ead15ede22aee67f1f5ed4aa8e48819/crates/gpui/src/elements/image_cache.rs#L280-L297
}

impl ImageCache for HashMapImageCache {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        HashMapImageCache::load(self, resource, window, cx)
    }
}
