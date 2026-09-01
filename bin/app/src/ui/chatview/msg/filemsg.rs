/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! The fud file-message type node.
//!
//! File messages are derived from privmsg text containing fud URLs —
//! never stored, keyed `(privmsg ts, derived id)` so the box sorts
//! directly below its source line. Status and decoded images are
//! content-addressed state on the node, surviving instance release:
//! re-materialization attaches to current progress instead of
//! restarting. The download tasks themselves live in the fud plugin;
//! this node only requests them (via `download_request`) and renders
//! their progress (via `set_file_status`).

use async_lock::Mutex as AsyncMutex;
use async_trait::async_trait;
use darkfi_serial::{Decodable, Encodable, FutAsyncWriteExt, SerialDecodable, SerialEncodable};
use image::{ImageBuffer, Rgba};
use parking_lot::Mutex as SyncMutex;
use std::{
    collections::HashMap,
    io::Cursor,
    sync::{Arc, Weak},
};
use url::Url;

use crate::{
    gfx::{gfxtag, DrawInstruction, EpochTracker, Point, Rectangle, RenderApi, Renderer},
    mesh::{Color, MeshBuilder, COLOR_CYAN, COLOR_GREEN, COLOR_RED, COLOR_WHITE},
    prop::{Property, PropertyColor, PropertySubType, PropertyType, Role},
    scene::{CallArgType, Pimpl, SceneNode, SceneNodeType, SceneNodeWeak},
    text,
    ui::UIObject,
    util::i18n::I18nBabelFish,
};

use super::{DrawOutcome, Hit, SharedProps};
use crate::ui::chatview::{
    buffer::MsgBuffer, loader::Loader, ChatView, MessageId, MsgRecord, MsgType, Timestamp,
};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::chatview::filemsg", $($arg)*); } }
macro_rules! i { ($($arg:tt)*) => { info!(target: "ui::chatview::filemsg", $($arg)*); } }

/// The file transfer lifecycle of a fud file message.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub enum FileMsgStatus {
    Initializing,
    Idle,
    Downloading { progress: f32 },
    Downloaded { path: String },
    Error { msg: String, progress: f32 },
}

type GenericImageBuffer = ImageBuffer<Rgba<u8>, Vec<u8>>;

/// Content-addressed state for one file URL: the download status and,
/// once decoded, the image. Survives instance release (eviction) —
/// re-materialization attaches to this instead of restarting.
pub struct FileContent {
    pub status: FileMsgStatus,
    pub imgbuf: Option<GenericImageBuffer>,
}

struct FileInner {
    instances: HashMap<super::privmsg::InstKey, FileMsgInstance>,
    touches: HashMap<super::privmsg::InstKey, u64>,
    access: u64,
    epoch_tracker: Option<EpochTracker>,
}

/// The per-message rendered cache: layouts for the status lines, the
/// active (click-to-download) rect, cached draw instructions.
pub struct FileMsgInstance {
    url: Url,
    lines: Vec<text::TextLayout>,
    line_height: f32,
    max_width: f32,
    status_strs: Vec<String>,
    active_rect: Option<Rectangle>,
    instrs: Option<Vec<DrawInstruction>>,
    height: f32,
}

pub type FileMsgNodePtr = Arc<FileMsgNode>;

pub struct FileMsgNode {
    node: SceneNodeWeak,
    shared: SharedProps,
    i18n: I18nBabelFish,
    loader: Arc<Loader>,
    buffer: Arc<AsyncMutex<MsgBuffer>>,
    chat: Weak<ChatView>,
    inner: SyncMutex<FileInner>,
    content: SyncMutex<HashMap<Url, FileContent>>,
}

/// Extract the first fud URL from a privmsg body, if any.
pub fn get_file_url(text: &str) -> Option<Url> {
    let re = regex::Regex::new(r"fud://[^\s]+").unwrap();
    re.find(text).and_then(|m| Url::parse(m.as_str()).ok())
}

/// The synthetic id of a file message derived from its privmsg: a
/// domain-separated hash with the top byte forced high, so the box
/// sorts directly below its source line (older in display order).
pub fn derived_file_id(source: &MessageId) -> MessageId {
    let mut hash = blake3::hash(&source.0).as_bytes()[..8].to_vec();
    hash[0] = 0xff;
    let mut id = [0u8; 32];
    id[..8].copy_from_slice(&hash);
    MessageId(id)
}

/// Encode a file message payload: the fud URL.
pub fn encode_filemsg_payload(url: &Url) -> Vec<u8> {
    let mut payload = vec![];
    url.to_string().encode(&mut payload).unwrap();
    payload
}

/// Decode a file message payload back into its URL.
///
/// ## Panics
///
/// If the payload does not decode, identifying the entry.
pub fn decode_filemsg_payload(payload: &[u8], ts: Timestamp, id: &MessageId) -> Url {
    let url: String = String::decode(&mut Cursor::new(payload))
        .unwrap_or_else(|e| panic!("corrupt chat entry: bad filemsg url [ts={ts} id={id}]: {e}"));
    Url::parse(&url).unwrap_or_else(|e| panic!("corrupt chat entry: bad filemsg url [{url}]: {e}"))
}

/// Build the derived filemsg record for a privmsg record, if its text
/// carries a fud URL.
pub fn derive_filemsg(privmsg: &MsgRecord, text: &str) -> Option<MsgRecord> {
    let url = get_file_url(text)?;
    Some(MsgRecord {
        ts: privmsg.ts,
        id: derived_file_id(&privmsg.id),
        msg_type: MsgType::FileMsg,
        payload: encode_filemsg_payload(&url),
        height: 0.,
    })
}

impl FileMsgNode {
    pub async fn new(
        node: SceneNodeWeak,
        shared: SharedProps,
        i18n: I18nBabelFish,
        loader: Arc<Loader>,
        buffer: Arc<AsyncMutex<MsgBuffer>>,
        chat: Weak<ChatView>,
    ) -> Pimpl {
        let self_ = Arc::new(Self {
            node: node.clone(),
            shared,
            i18n,
            loader,
            buffer,
            chat,
            inner: SyncMutex::new(FileInner {
                instances: HashMap::new(),
                touches: HashMap::new(),
                access: 0,
                epoch_tracker: None,
            }),
            content: SyncMutex::new(HashMap::new()),
        });
        Pimpl::FileMsgNode(self_)
    }

    /// The (translated) status line for a status. Fluent keys are the
    /// stable ids below; untranslated ids fall back to English.
    fn status_str(&self, status: &FileMsgStatus) -> String {
        let fallback = |id: &str, english: &str| {
            self.i18n
                .tr(&format!("chatview-file-status-{id}"))
                .unwrap_or_else(|| english.to_string())
        };
        match status {
            FileMsgStatus::Initializing => fallback("initializing", "starting fud"),
            FileMsgStatus::Idle => fallback("idle", "tap to download"),
            FileMsgStatus::Downloading { progress } => {
                format!("{} [{progress:.1}%]", fallback("downloading", "downloading"))
            }
            FileMsgStatus::Downloaded { .. } => fallback("downloaded", "downloaded"),
            FileMsgStatus::Error { msg, progress } => {
                let msg = msg.to_lowercase();
                if *progress > 0. {
                    format!("{msg} [{progress:.1}%]")
                } else {
                    msg
                }
            }
        }
    }

    /// The two box lines: shortened file hash and the status string.
    fn file_strs(&self, url: &Url, status: &FileMsgStatus) -> Vec<String> {
        let hash = url.host_str().unwrap_or("???");
        let short = if hash.chars().count() >= 12 {
            let head: String = hash.chars().take(4).collect();
            let tail: String = hash.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
            format!("{head}...{tail}")
        } else {
            hash.to_string()
        };
        vec![short, self.status_str(status)]
    }

    fn status_color(status: &FileMsgStatus, timestamp_color: Color) -> Color {
        match status {
            FileMsgStatus::Initializing => timestamp_color,
            FileMsgStatus::Idle => timestamp_color,
            FileMsgStatus::Downloading { .. } => COLOR_CYAN,
            FileMsgStatus::Downloaded { .. } => COLOR_GREEN,
            FileMsgStatus::Error { .. } => COLOR_RED,
        }
    }

    fn load_img(path: &str) -> Option<GenericImageBuffer> {
        let data = Arc::new(SyncMutex::new(vec![]));
        let data2 = data.clone();
        miniquad::fs::load_file(path, move |res| {
            if let Ok(res) = res {
                *data2.lock() = res;
            }
        });
        let data = std::mem::take(&mut *data.lock());
        let img =
            image::ImageReader::new(Cursor::new(data)).with_guessed_format().ok()?.decode().ok()?;
        Some(img.to_rgba8())
    }

    fn img_size(&self, imgbuf: &GenericImageBuffer) -> (f32, f32) {
        const IMG_MAX_HEIGHT: f32 = 500.;
        let max_width = self.shared.rect.get().w - self.shared.timestamp_width.get();
        let img_w = imgbuf.width() as f32;
        let img_h = imgbuf.height() as f32;
        let scale = (max_width / img_w).min(IMG_MAX_HEIGHT / img_h);
        (img_w * scale, img_h * scale)
    }

    /// Measure a record: materialize if needed, return the height.
    pub fn measure(&self, rec: &MsgRecord) -> f32 {
        let key = (rec.ts, rec.id);
        let mut inner = self.inner.lock();
        inner.access += 1;
        let access = inner.access;
        inner.touches.insert(key, access);

        if inner.instances.contains_key(&key) {
            let inst = inner.instances.get(&key).unwrap();
            return inst.height
        }

        let url = decode_filemsg_payload(&rec.payload, rec.ts, &rec.id);
        let height = {
            let mut content = self.content.lock();
            let entry = content.entry(url.clone()).or_insert_with(|| FileContent {
                status: FileMsgStatus::Initializing,
                imgbuf: None,
            });
            if entry.status == FileMsgStatus::Initializing {
                // First sight: idle until a download is requested.
                entry.status = FileMsgStatus::Idle;
            }
            if let Some(imgbuf) = &entry.imgbuf {
                let (_, img_h) = self.img_size(imgbuf);
                img_h + Self::MARGIN_TOP + Self::MARGIN_BOTTOM + self.shared.message_spacing.get()
            } else {
                self.box_height()
            }
        };

        let inst = FileMsgInstance {
            url,
            lines: vec![],
            line_height: self.shared.line_height.get(),
            max_width: 0.,
            status_strs: vec![],
            active_rect: None,
            instrs: None,
            height,
        };
        inner.instances.insert(key, inst);
        t!("materialized id={} height={height}", rec.id);
        height
    }

    const MARGIN_TOP: f32 = 4.;
    const MARGIN_BOTTOM: f32 = 10.;

    /// The status box height.
    fn box_height(&self) -> f32 {
        const BOX_PADDING_Y: f32 = 12.;
        let line_height = self.shared.line_height.get();
        2. * line_height +
            BOX_PADDING_Y * 2. +
            Self::MARGIN_TOP +
            Self::MARGIN_BOTTOM +
            self.shared.message_spacing.get()
    }

    /// Whether the instance currently holds rendered state.
    pub fn is_materialized(&self, rec: &MsgRecord) -> bool {
        self.inner.lock().instances.contains_key(&(rec.ts, rec.id))
    }

    /// Drop an instance's rendered state; content-addressed state
    /// survives for re-materialization to attach to.
    pub fn release(&self, key: &super::privmsg::InstKey) {
        let mut inner = self.inner.lock();
        inner.instances.remove(key);
        inner.touches.remove(key);
    }

    /// Drop every instance's rendered state.
    pub fn release_all(&self) {
        let mut inner = self.inner.lock();
        inner.instances.clear();
        inner.touches.clear();
    }

    /// Rebuild rendered state from live props + current data.
    pub fn regen(&self, key: &super::privmsg::InstKey) {
        self.release(key);
    }

    /// Rebuild every instance's rendered state.
    pub fn regen_all(&self) {
        self.release_all();
    }

    /// Release the out-of-window instances beyond the LRU budget.
    pub fn sweep(&self, keep: &std::collections::HashSet<super::privmsg::InstKey>, budget: usize) {
        let releases = {
            let inner = self.inner.lock();
            super::evict_beyond(keep, &inner.touches, budget)
        };
        for key in releases {
            self.release(&key);
        }
    }

    /// Renderer-bound draw instructions in message-local coordinates.
    pub fn draw(&self, rec: &MsgRecord, renderer: &Renderer) -> DrawOutcome {
        const BOX_PADDING_Y: f32 = 12.;
        const BOX_PADDING_X: f32 = 15.;
        const GLOW_SIZE: f32 = 20.;

        let key = (rec.ts, rec.id);
        let mut inner = self.inner.lock();
        inner.access += 1;
        let access = inner.access;
        inner.touches.insert(key, access);
        let epoch_changed =
            inner.epoch_tracker.get_or_insert_with(|| EpochTracker::new(renderer)).changed();
        if epoch_changed {
            for inst in inner.instances.values_mut() {
                inst.instrs = None;
            }
        }

        let Some(inst) = inner.instances.get_mut(&key) else { return DrawOutcome::Inline(vec![]) };
        let line_height = self.shared.line_height.get();
        let timestamp_width = self.shared.timestamp_width.get();
        let timestamp_color = self.shared.timestamp_color.get();
        let font_size = self.shared.font_size.get();
        let window_scale = self.shared.window_scale.get();
        let max_width = self.shared.rect.get().w - timestamp_width - GLOW_SIZE;

        if inst.instrs.is_none() {
            let (status, imgbuf) = {
                let content = self.content.lock();
                content
                    .get(&inst.url)
                    .map(|c| (c.status.clone(), c.imgbuf.clone()))
                    .unwrap_or_else(|| (FileMsgStatus::Initializing, None))
            };

            let mut instrs = vec![];

            if let Some(imgbuf) = imgbuf {
                // Downloaded image: fitted to bounds, with a glow.
                let (img_w, img_h) = self.img_size(&imgbuf);
                let mesh_rect = Rectangle::from([timestamp_width, Self::MARGIN_TOP, img_w, img_h]);
                let width = imgbuf.width() as u16;
                let height = imgbuf.height() as u16;
                let bmp = imgbuf.as_raw().clone();
                let texture = renderer.new_texture(
                    width,
                    height,
                    bmp,
                    miniquad::TextureFormat::RGBA8,
                    gfxtag!("chatview_fileimg_texture"),
                );
                let mut mesh_gradient = MeshBuilder::new(gfxtag!("chatview_fileimg_glow"));
                let glow_color = [timestamp_color[0], timestamp_color[1], timestamp_color[2], 0.5];
                mesh_gradient.draw_box_shadow(&mesh_rect, glow_color, GLOW_SIZE);
                instrs.push(DrawInstruction::Draw(mesh_gradient.alloc(renderer).draw_untextured()));
                let mut mesh_img = MeshBuilder::new(gfxtag!("chatview_fileimg"));
                let uv_rect = Rectangle::from([0., 0., 1., 1.]);
                mesh_img.draw_box(&mesh_rect, COLOR_WHITE, &uv_rect);
                instrs.push(DrawInstruction::Draw(
                    mesh_img.alloc(renderer).draw_with_textures(vec![texture]),
                ));
                inst.active_rect = Some(mesh_rect);
            } else {
                // Status box: outline + glow + the two text lines.
                let color = Self::status_color(&status, timestamp_color);
                let file_strs = self.file_strs(&inst.url, &status);
                let mut layouts = Vec::with_capacity(file_strs.len());
                let mut text_width = 0.;
                for file_str in &file_strs {
                    let layout = text::make_layout(
                        file_str,
                        color,
                        font_size,
                        line_height / font_size,
                        window_scale,
                        Some(max_width),
                        &[],
                    );
                    if layout.width() > text_width {
                        text_width = layout.width();
                    }
                    layouts.push(layout);
                }
                inst.status_strs = file_strs;

                let box_height = 2. * line_height + BOX_PADDING_Y * 2.;
                let box_width = if text_width > max_width { max_width } else { text_width } +
                    BOX_PADDING_X * 2.;
                let mesh_rect =
                    Rectangle::new(timestamp_width, Self::MARGIN_TOP, box_width, box_height);

                let mut mesh = MeshBuilder::new(gfxtag!("chatview_filemsg_box"));
                mesh.draw_outline(&mesh_rect, color, 1.);
                let glow_color = [color[0], color[1], color[2], 0.3];
                mesh.draw_box_shadow(&mesh_rect, glow_color, GLOW_SIZE);
                instrs.push(DrawInstruction::Draw(mesh.alloc(renderer).draw_untextured()));

                instrs.push(DrawInstruction::Move(Point::new(
                    timestamp_width + BOX_PADDING_X,
                    Self::MARGIN_TOP + BOX_PADDING_Y,
                )));
                for layout in layouts {
                    let text_instrs =
                        text::render_layout(&layout, renderer, gfxtag!("chatview_filemsg_text"));
                    instrs.extend(text_instrs);
                    instrs.push(DrawInstruction::Move(Point::new(0., line_height)));
                }
                inst.active_rect = Some(mesh_rect);
                inst.lines = vec![];
            }

            inst.instrs = Some(instrs);
        }
        DrawOutcome::Inline(inst.instrs.clone().unwrap_or_default())
    }

    /// Clipboard contribution when selected: the file URL.
    pub fn copy_text(&self, rec: &MsgRecord) -> Option<String> {
        let inner = self.inner.lock();
        inner.instances.get(&(rec.ts, rec.id)).map(|inst| inst.url.to_string())
    }

    /// Hit dispatch: the active rect (image or status box) activates a
    /// download request when idle or errored.
    pub fn hit_test(&self, rec: &MsgRecord, pos: Point) -> Option<Hit> {
        let inner = self.inner.lock();
        let inst = inner.instances.get(&(rec.ts, rec.id))?;
        let rect = inst.active_rect?;
        if !rect.contains(pos) {
            return None
        }
        let status = {
            let content = self.content.lock();
            content.get(&inst.url).map(|c| c.status.clone()).unwrap_or(FileMsgStatus::Initializing)
        };
        match status {
            FileMsgStatus::Idle | FileMsgStatus::Error { .. } => Some(Hit::File(inst.url.clone())),
            _ => None,
        }
    }

    /// Update the status of every file message with this URL; heights
    /// re-flow into geometry with scroll compensation, `status_changed`
    /// fires for each affected message, and a finished download decodes
    /// its image into the content store.
    pub async fn set_file_status(&self, url: &Url, status: FileMsgStatus) {
        t!("set_file_status({url}, {status:?})");

        {
            let mut content = self.content.lock();
            let Some(entry) = content.get_mut(url) else { return };
            if entry.status != status {
                entry.status = status.clone();
                if let FileMsgStatus::Downloaded { path } = &status {
                    entry.imgbuf = Self::load_img(path);
                    t!("decoded image for {url}: {}", entry.imgbuf.is_some());
                }
            }
        }

        // Regen every loaded record carrying this URL and flow the new
        // heights into geometry.
        let mut buffer = self.buffer.lock().await;
        let mut keys = vec![];
        for rec in buffer.iter_display_order() {
            if rec.msg_type == MsgType::FileMsg {
                let rec_url = decode_filemsg_payload(&rec.payload, rec.ts, &rec.id);
                if &rec_url == url {
                    keys.push((rec.ts, rec.id));
                }
            }
        }
        drop(buffer);

        for key in keys {
            self.regen(&key);
            let rec = {
                let buffer = self.buffer.lock().await;
                buffer.record(&key.1).filter(|r| r.ts == key.0).cloned()
            };
            let Some(rec) = rec else { continue };
            let height = self.measure(&rec);

            let mut buffer = self.buffer.lock().await;
            let below = match buffer.pos_of(&key.1) {
                Some(top) => {
                    let scroll = self.controller_scroll();
                    top <= scroll
                }
                None => false,
            };
            if let Some(delta) = buffer.set_height_key(&key, height) {
                if let Some(chat) = self.chat.upgrade() {
                    let mut ctl = chat.controller.lock();
                    ctl.compensate(delta, below);
                }
            }

            if let Some(node) = self.node.upgrade() {
                let mut data = vec![];
                key.1.encode(&mut data).unwrap();
                let _ = node.trigger("status_changed", data).await;
            }
        }

        if let Some(chat) = self.chat.upgrade() {
            chat.redraw.trigger();
        }
    }

    fn controller_scroll(&self) -> f32 {
        self.chat.upgrade().map(|chat| chat.controller.lock().scroll()).unwrap_or(0.)
    }

    /// Request the download of a file message: emits
    /// `download_request(id, url)`.
    pub async fn request_download(&self, id: &MessageId, url: &Url) {
        t!("download requested: {url}");
        if let Some(node) = self.node.upgrade() {
            let mut data = vec![];
            id.encode(&mut data).unwrap();
            url.encode(&mut data).unwrap();
            let _ = node.trigger("download_request", data).await;
        }
    }

    /// The scene node handle.
    pub fn node(&self) -> &SceneNodeWeak {
        &self.node
    }

    /// The content state of a file URL (test access).
    pub(crate) fn status_of(&self, url: &Url) -> Option<FileMsgStatus> {
        self.content.lock().get(url).map(|c| c.status.clone())
    }

    /// The box lines for a status (test access).
    pub(crate) fn file_strs_for_test(&self, url: &Url, status: &FileMsgStatus) -> Vec<String> {
        self.file_strs(url, status)
    }
}

/// Scene node factory for the file-message type node.

#[async_trait]
impl UIObject for FileMsgNode {
    fn priority(&self) -> u32 {
        0
    }
}

impl std::fmt::Debug for FileMsgNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.node.upgrade())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::node::create_filemsg_node, prop::PropertyAtomicGuard, ui::chatview::codec};

    async fn make_node(tag: &str, i18n_src: &str) -> (FileMsgNodePtr, Arc<AsyncMutex<MsgBuffer>>) {
        let chat = crate::app::node::create_chatview("chatview");
        let chat = chat.setup_null();
        let atom = &mut PropertyAtomicGuard::none();
        chat.set_property_f32(atom, Role::App, "font_size", 14.).unwrap();
        chat.set_property_f32(atom, Role::App, "timestamp_font_size", 10.).unwrap();
        chat.set_property_f32(atom, Role::App, "timestamp_width", 50.).unwrap();
        chat.set_property_f32(atom, Role::App, "line_height", 20.).unwrap();
        chat.set_property_f32(atom, Role::App, "message_spacing", 4.).unwrap();
        let prop = chat.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_f32(atom, Role::App, 2, 800.).unwrap();
        prop.set_f32(atom, Role::App, 3, 600.).unwrap();
        let prop = chat.get_property("timestamp_color").unwrap();
        for (i, c) in [0.5, 0.5, 0.5, 1.].iter().enumerate() {
            prop.set_f32(atom, Role::App, i, *c).unwrap();
        }

        let mut wscale = crate::scene::SceneNode::new("w", crate::scene::SceneNodeType::Object);
        wscale
            .add_property(Property::new("scale", PropertyType::Float32, PropertySubType::Null))
            .unwrap();
        let wscale = wscale.setup_null();
        wscale.set_property_f32(atom, Role::App, "scale", 1.).unwrap();
        let window_scale =
            crate::prop::PropertyFloat32::wrap(&wscale, Role::Internal, "scale", 0).unwrap();

        let shared = super::super::SharedProps::wrap(&chat, window_scale);
        let mut raw = MsgBuffer::new();
        raw.disable_separators();
        let buffer = Arc::new(AsyncMutex::new(raw));
        let (redraw, _rx) = crate::ui::RedrawTrigger::new();
        let loader = Loader::new(buffer.clone(), redraw);

        let i18n = I18nBabelFish::new(i18n_src.to_string(), "en-US");
        let chat_weak: Weak<ChatView> = Weak::new();

        let node = create_filemsg_node("filemsg");
        let shared2 = shared.clone();
        let i18n2 = i18n.clone();
        let loader2 = loader.clone();
        let buffer2 = buffer.clone();
        let node = node
            .setup(|me| async move {
                FileMsgNode::new(me, shared2, i18n2, loader2, buffer2, chat_weak).await
            })
            .await;
        chat.link(node.clone());
        let Pimpl::FileMsgNode(ptr) = node.pimpl() else { panic!() };
        (ptr.clone(), buffer)
    }

    fn file_rec(ts: Timestamp, idb: u8, url: &Url) -> MsgRecord {
        let mut id = [0u8; 32];
        id[0] = idb;
        MsgRecord {
            ts,
            id: MessageId(id),
            msg_type: MsgType::FileMsg,
            payload: encode_filemsg_payload(url),
            height: 0.,
        }
    }

    #[test]
    fn derivation_keys_and_orders() {
        let payload =
            codec::encode_privmsg_payload("alice", "grab fud://abcdef/file.tar now", true);
        let privmsg = MsgRecord {
            ts: 1000,
            id: MessageId([7; 32]),
            msg_type: MsgType::PrivMsg,
            payload,
            height: 0.,
        };

        let file = derive_filemsg(&privmsg, "grab fud://abcdef/file.tar now").expect("derived");
        assert_eq!(file.ts, privmsg.ts, "shares the privmsg timestamp");
        assert!(file.id.0 > privmsg.id.0, "sorts directly below its source line");
        assert_eq!(file.msg_type, MsgType::FileMsg);
        let url = decode_filemsg_payload(&file.payload, file.ts, &file.id);
        assert_eq!(url.host_str(), Some("abcdef"));

        assert!(derive_filemsg(&privmsg, "no urls here").is_none());
    }

    #[test]
    fn statuses_measured_and_content_survives_release() {
        let (node, _buffer) = smol::block_on(make_node("status", ""));
        let url = Url::parse("fud://abcdef012345/file.png").unwrap();
        let rec = file_rec(1000, b'a', &url);

        let h1 = node.measure(&rec);
        // 2 text lines + paddings + margins + spacing.
        assert!((h1 - (2. * 20. + 12. * 2. + 4. + 10. + 4.)).abs() < 0.01, "{h1}");

        // First sight registers Idle content state.
        assert_eq!(node.status_of(&url), Some(FileMsgStatus::Idle));

        // A status update lands in the content store and regens.
        smol::block_on(async {
            node.set_file_status(&url, FileMsgStatus::Downloading { progress: 42. }).await;
        });
        assert_eq!(node.status_of(&url), Some(FileMsgStatus::Downloading { progress: 42. }));

        // Eviction drops the instance; the content-addressed state
        // survives and re-materialization attaches to it.
        node.release(&(rec.ts, rec.id));
        assert!(!node.is_materialized(&rec));
        assert_eq!(node.status_of(&url), Some(FileMsgStatus::Downloading { progress: 42. }));
        let h2 = node.measure(&rec);
        assert_eq!(h1, h2, "same status box height");
    }

    #[test]
    fn status_strings_translate() {
        let (node, _buffer) = smol::block_on(make_node(
            "i18n",
            "chatview-file-status-idle = zum Herunterladen tippen\n",
        ));
        let url = Url::parse("fud://abcdef012345/file.png").unwrap();
        let rec = file_rec(1000, b'a', &url);
        node.measure(&rec);

        let strs = node.file_strs_for_test(&url, &FileMsgStatus::Idle);
        assert_eq!(strs[1], "zum Herunterladen tippen");

        // Untranslated statuses fall back to the English string.
        let strs =
            node.file_strs_for_test(&url, &FileMsgStatus::Downloaded { path: String::new() });
        assert_eq!(strs[1], "downloaded");
    }
}
