mod cache;
mod seventv;

use gpui::{
    App, AppContext, Application, Bounds, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, KeyBinding, LayoutId, MouseButton, MouseUpEvent,
    PaintQuad, Pixels, ShapedLine, SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window, WindowBounds,
    WindowOptions, actions, black, div, fill, hsla, image_cache, img, point, prelude::*, px, relative, rgb, rgba, size,
};
use image::{AnimationDecoder, DynamicImage, Rgba, codecs::webp::WebPDecoder};
use lazy_static::lazy_static;
use seventv::WebmEmote;
use std::collections::VecDeque;
use std::env;
use std::fs::{self, File};
use std::io::{BufReader, Cursor, prelude::*};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, atomic};
use std::time::Duration;
use unicode_segmentation::*;
use util::truncate_to_byte_limit;
use wl_clipboard_rs::copy::{ClipboardType, MimeSource, MimeType, Options, Source};

actions!(text_input, [Backspace, Escape, CtrlSpace, CtrlS]);

lazy_static! {
    static ref APP_NAME: String = String::from(if cfg!(debug_assertions) { "dev-kemote" } else { "kemote" });
    static ref CACHE_DIR: String = format!("{}/.cache/{}", env::var("HOME").unwrap(), *APP_NAME);
}
const VERSION: &str = "0.2.0"; // keep in sync with Cargo.toml

#[derive(Debug, Clone)]
struct DisplayedEmote {
    emote: seventv::WebmEmote,
}

impl DisplayedEmote {
    fn on_mouse_up(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        println!("CLICKED EMOTE: {:?}", &self.emote);

        let window_root = window
            .root::<InputExample>()
            .expect("rip unwrap 1")
            .expect("rip unwrap 2");

        window_root.update(cx, |view, cx| {
            view.text_input.update(cx, |tinput, _cx| {
                // @TODO: To make window re-render recently clicked emotes, we would need to
                //  actually have some way to track if those are currently being rendered vs
                //  specific search emojies being rendered. The simplest way would probably be a
                //  'recent emotes' state (bool?) so instead of copying recent emotes into emotes
                //  array, we would just render from emotes array if recent emotes is true. Only
                //  then this change could be updated in real time.
                tinput.recent_emotes.access(self.emote.clone());
                println!("NEW SIZE OF RECENT EMOTES: {:?}", tinput.recent_emotes.emotes.len());
            });
        });

        let f = File::open(PathBuf::from(WebmEmote::path(&self.emote.url))).expect("rip opening emotes path");
        let mut webp_decoder = WebPDecoder::new(BufReader::new(f)).expect("rip webp decoder");
        let mut buffer: Vec<u8> = Vec::new();

        let final_path: String;
        if webp_decoder.has_animation() {
            webp_decoder
                .set_background_color(Rgba([0, 0, 0, 0]))
                .expect("rip webp decoder");
            webp_decoder
                .into_frames()
                // @TODO: we take first frame right now, but we should allow getting static webp
                //  sticker out of any frame that you picked by clicking at the played webp, but
                //  for that obviously need to get a webp player state frame-perfect during this
                //  execution. This would be really nice though and much more usable than current.
                .next()
                .expect("rip no first webp frame")
                .expect("rip error on first webp frame")
                .buffer()
                .write_to(&mut Cursor::new(&mut buffer), image::ImageFormat::Png)
                .expect("rip convert and write to buffer");

            // @TODO: We have to create a tmp fake file with the single frame for telegram to recognize
            //  the format correctly. I'm not sure how hackable it is, we could try to fake it and lie
            //  to telegram somehow, but this is probably fine for the future. In the future might want
            //  to create this file in the persistent cache during download.
            final_path = format!("/tmp/{}.webp", self.emote.id.clone());
            File::create(final_path.clone())
                .expect("rip tmp file")
                .write_all(&buffer)
                .expect("rip write file");
        } else {
            final_path = WebmEmote::path(&self.emote.url);
            DynamicImage::from_decoder(webp_decoder)
                .expect("rip decoding static webp")
                .to_rgba8()
                .write_to(&mut Cursor::new(&mut buffer), image::ImageFormat::Png)
                .expect("rip write bytes");
        }

        let mut opts = Options::new();
        opts.omit_additional_text_mime_types(true); // do not add default mimetypes
        opts.clipboard(ClipboardType::Both);
        opts.copy_multi(vec![
            MimeSource {
                source: Source::Bytes(buffer.into()),
                mime_type: MimeType::Specific("image/webp".to_string()),
            },
            MimeSource {
                source: Source::Bytes(format!("file://{}", &final_path).into_bytes().into()),
                mime_type: MimeType::Specific("text/x-moz-url".to_string()),
            },
        ])
        .expect("rip multi-copy into clipboard");
    }
}

impl Render for DisplayedEmote {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w_20()
            .ml_6()
            .mt_6()
            .text_center()
            .child(
                img(self.emote.url.clone())
                    .max_w_20()
                    .max_h_20()
                    .object_fit(gpui::ObjectFit::Contain)
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
                    .id("webp")
                    // Default loading element (hardcoded emote for now) ...
                    .with_loading(|| {
                        img("https://cdn.7tv.app/emote/01F79PC23G0000DRDGH5T4QFMA/4x.webp")
                            .max_w_20()
                            .max_h_20()
                            .object_fit(gpui::ObjectFit::Contain)
                            .id("webp")
                            .into_any_element()
                    }),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .flex_shrink_0()
                    .overflow_x_hidden()
                    .text_ellipsis()
                    .line_clamp(1)
                    .child(self.emote.name.clone()),
            )
    }
}

#[derive(Debug)]
struct RecentEmotes {
    emotes: VecDeque<seventv::WebmEmote>,
    capacity: usize,
}

impl RecentEmotes {
    pub fn new(capacity: usize) -> Self {
        let mut recent_emotes = Self {
            emotes: VecDeque::with_capacity(capacity),
            capacity,
        };

        let recent_fp = format!("{}/recent.json", *CACHE_DIR);
        if let Ok(mut file) = File::open(recent_fp.clone()) {
            let mut contents = String::new();
            file.read_to_string(&mut contents).expect("rip file read");
            let emotes: Vec<seventv::WebmEmote> = serde_json::from_str(&contents).expect("rip json load");
            for emote in emotes {
                recent_emotes.emotes.push_back(emote);
            }
        }

        recent_emotes
    }

    pub fn access(&mut self, emote: seventv::WebmEmote) {
        if let Some(pos) = self.emotes.iter().position(|e| e == &emote) {
            self.emotes.remove(pos);
        }

        if self.emotes.len() >= self.capacity {
            self.emotes.pop_back();
        }

        self.emotes.push_front(emote);

        let recent_fp = format!("{}/recent.json", *CACHE_DIR);
        if let Ok(mut file) = File::create(recent_fp.clone()) {
            file.write_all(serde_json::to_vec_pretty(&self.emotes).unwrap().as_ref())
                .expect("rip write file");
        }
    }

    pub fn recent(&self) -> impl Iterator<Item = &seventv::WebmEmote> {
        self.emotes.iter()
    }
}

struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    emotes: Vec<Entity<DisplayedEmote>>,
    recent_emotes: RecentEmotes,
    last_active: Arc<atomic::AtomicBool>,
}

impl TextInput {
    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn show_recent_emotes(&mut self, _: &CtrlSpace, _window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn(async move |entity, cx| {
            entity
                .update(cx, |new_self, cx| {
                    let emotes: Vec<seventv::WebmEmote> = new_self.recent_emotes.recent().cloned().collect();

                    new_self.emotes.clear();
                    for emote in emotes {
                        new_self.emotes.push(cx.new(|_cx| DisplayedEmote { emote }));
                    }
                    cx.notify();
                })
                .expect("rip updating text_input");
        })
        .detach();
    }

    fn clear_input(&mut self, _: &CtrlS, _window: &mut Window, cx: &mut Context<Self>) {
        self.reset();

        cx.spawn(async move |entity, cx| {
            entity
                .update(cx, |new_self, cx| {
                    let emotes: Vec<seventv::WebmEmote> = new_self.recent_emotes.recent().cloned().collect();

                    new_self.emotes.clear();
                    for emote in emotes {
                        new_self.emotes.push(cx.new(|_cx| DisplayedEmote { emote }));
                    }
                    cx.notify();
                })
                .expect("rip updating text_input");
        })
        .detach();

        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn reset(&mut self) {
        self.content = "".into();
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.marked_range = None;
        self.last_layout = None;
        self.last_bounds = None;
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        println!("text_for_range:entry - {:?}", actual_range.clone());
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        // println!("text_for_range:exit - {:?}, {:?}", range.clone(), actual_range.clone());
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        println!("selected_text_range:entry - ???");
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<Range<usize>> {
        println!("marked_text_range:entry - ???");
        self.marked_range.as_ref().map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        println!("unmark_text:entry - ???");
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        println!("replace_text_in_range:entry ---");
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content = (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..]).into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        cx.notify();

        // QUERY SECTION

        let query = truncate_to_byte_limit(&self.content, 64).to_lowercase();
        self.last_active.store(false, atomic::Ordering::Relaxed);
        self.last_active = Arc::new(atomic::AtomicBool::new(true));

        let last_active = self.last_active.clone();
        cx.spawn(async move |entity, cx| {
            cx.background_executor().timer(Duration::from_millis(200)).await;

            if !last_active.load(atomic::Ordering::Relaxed) {
                return;
            }

            println!("Potential query: {:?}", query);
            let mut emotes: Vec<seventv::WebmEmote> = vec![];
            if !query.is_empty() {
                // emotes = seventv::get_7tv(query.to_sanitized_string()).await;
                let queries_dir = format!("{}/queries", *CACHE_DIR);
                fs::create_dir_all(queries_dir.clone()).expect("rip queries dir");
                let query_fp = format!("{}/{}.json", queries_dir, sha256::digest(query.clone()));
                if let Ok(mut file) = File::open(query_fp.clone()) {
                    let mut contents = String::new();
                    file.read_to_string(&mut contents).expect("rip file read");
                    emotes = serde_json::from_str(&contents).expect("rip json load");
                } else {
                    println!("QUERYING: {:?}", query.clone());
                    emotes = seventv::query_7tv(query.to_string()).await;
                    let mut file = File::create(query_fp.clone()).expect("rip create file");
                    file.write_all(serde_json::to_vec_pretty(&emotes).unwrap().as_ref())
                        .expect("rip write file");
                }

                if !last_active.load(atomic::Ordering::Relaxed) {
                    return;
                }
            }

            entity
                .update(cx, |new_self, cx| {
                    if !last_active.load(atomic::Ordering::Relaxed) {
                        return;
                    }

                    if query.is_empty() {
                        emotes = new_self.recent_emotes.recent().cloned().collect();
                    }

                    new_self.emotes.clear();
                    for emote in emotes {
                        new_self.emotes.push(cx.new(|_cx| DisplayedEmote { emote }));
                    }
                    cx.notify();
                })
                .expect("rip updating text_input");
        })
        .detach();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        println!("replace_and_mark_text_in_range:entry - ???");
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content = (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..]).into();
        self.marked_range = Some(range.start..range.start + new_text.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        println!("bounds_for_range:entry - ???");
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(bounds.left() + last_layout.x_for_index(range.start), bounds.top()),
            point(bounds.left() + last_layout.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        println!("character_index_for_print:entry - ???");
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;

        assert_eq!(last_layout.text, self.content);
        let utf8_index = last_layout.index_for_x(point.x - line_point.x)?;
        Some(self.offset_to_utf16(utf8_index))
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();

    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = &input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();

        let (display_text, text_color) = if content.is_empty() {
            (input.placeholder.clone(), hsla(0., 0., 0., 0.2))
        } else {
            (content.clone(), style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = input.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run.clone()
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window.text_system().shape_line(display_text, font_size, &runs).unwrap();

        let cursor_pos = line.x_for_index(cursor);
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_pos, bounds.top()),
                        size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    gpui::blue(),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(bounds.left() + line.x_for_index(selected_range.start), bounds.top()),
                        point(bounds.left() + line.x_for_index(selected_range.end), bounds.bottom()),
                    ),
                    rgba(0x3311ff30),
                )),
                None,
            )
        };
        PrepaintState {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(&focus_handle, ElementInputHandler::new(bounds, self.input.clone()), cx);
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection)
        }
        let line = prepaint.line.take().unwrap();
        line.paint(bounds.origin, window.line_height(), window, cx).unwrap();

        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .mt_6()
            .ml_auto()
            .mr_auto()
            .key_context("TextInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::show_recent_emotes))
            .on_action(cx.listener(Self::clear_input))
            .bg(rgb(0x838ba7))
            .line_height(px(30.))
            .text_size(px(24.))
            .w(px(320.))
            .child(
                div()
                    .h(px(30. + 4. * 2.))
                    .w(px(320.))
                    .p(px(4.))
                    .bg(rgb(0x838ba7))
                    .overflow_x_hidden()
                    .border_color(gpui::blue())
                    .child(TextElement {
                        input: cx.entity().clone(),
                    }),
            )
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

struct InputExample {
    text_input: Entity<TextInput>,
    image_cache: Entity<cache::HashMapImageCache>,
}

impl InputExample {
    fn exit(&mut self, _: &Escape, _window: &mut Window, _cx: &mut Context<Self>) {
        std::process::exit(0); // couldn't find any more proper way to close window
    }
}

impl Render for InputExample {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        image_cache(self.image_cache.clone()).size_full().child(
            div()
                .bg(rgb(0x626880))
                .on_action(cx.listener(Self::exit))
                .flex()
                .flex_col()
                .size_full()
                .child(
                    div()
                        .bg(rgb(0x414559))
                        .border_b_1()
                        .border_color(black())
                        .flex()
                        .flex_row()
                        .justify_between()
                        .child(
                            div()
                                .ml_auto()
                                .mr_auto()
                                .text_color(rgb(0xc6d0f5))
                                .child(format!("{} - v{}", *APP_NAME, VERSION)),
                        ),
                )
                .child(self.text_input.clone())
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .mr_6()
                        .mt_6()
                        .children(self.text_input.read(cx).emotes.iter().map(|gif| gif.clone())),
                )
                .child(
                    div()
                        .bg(rgb(0x414559))
                        .border_b_1()
                        .mt_auto()
                        .border_color(black())
                        .child(
                            div()
                                .ml_2()
                                .text_color(rgb(0xc6d0f5))
                                .flex()
                                .flex_row()
                                .gap_6()
                                .child(format!("esc: Exit"))
                                .child(format!("ctrl-space: Recent Emotes"))
                                .child(format!("ctrl-s: Clear Search")),
                        ),
                ),
        )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.activate(true);

        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, None),
            KeyBinding::new("escape", Escape, None),
            KeyBinding::new("ctrl-space", CtrlSpace, None),
            KeyBinding::new("ctrl-s", CtrlS, None),
        ]);

        // width: 24 + (80 * 10 + 24 * 9) + 24 = 1064
        // height: TODO
        let bounds = Bounds::centered(None, size(px(1064.), px(850.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, cx| {
                    let recent_emotes = RecentEmotes::new(15);
                    cx.new(|cx| InputExample {
                        text_input: cx.new(|cx| TextInput {
                            focus_handle: cx.focus_handle(),
                            content: "".into(),
                            placeholder: "Type here...".into(),
                            selected_range: 0..0,
                            selection_reversed: false,
                            marked_range: None,
                            last_layout: None,
                            last_bounds: None,
                            emotes: recent_emotes
                                .recent()
                                .map(|emote| cx.new(|_cx| DisplayedEmote { emote: emote.clone() }))
                                .collect(),
                            recent_emotes,
                            last_active: Arc::new(atomic::AtomicBool::new(true)),
                        }),
                        image_cache: cache::HashMapImageCache::new(cx),
                    })
                },
            )
            .unwrap();

        // This just sets focus to the input field when the window opens for the first time.
        window
            .update(cx, |view, window, cx| {
                window.set_window_title(&APP_NAME);
                window.set_app_id(&APP_NAME);

                window.focus(&view.text_input.focus_handle(cx));
            })
            .unwrap();
    });
}
