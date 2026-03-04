#![recursion_limit = "256"]

mod game;
mod render;

use crate::game::Game;
use crate::render::Renderer;

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use web_sys::{
    Document, Element, Event, HtmlElement, HtmlSelectElement, KeyboardEvent, PointerEvent, Window,
};

const DAS_MS: f64 = 140.0; // Delayed Auto Shift
const ARR_MS: f64 = 40.0; // Auto Repeat Rate
const GUIDANCE_LANG_EN: &str = "en";
// Unified list of clickable controls (top-bar + touch bar + modal action).
const BUTTON_IDS: [&str; 12] = [
    "btn-guidance-close",
    "btn-left",
    "btn-right",
    "btn-down",
    "btn-rot-cw",
    "btn-rot-ccw",
    "btn-drop",
    "btn-hold",
    "btn-input-mode",
    "btn-open-guidance",
    "btn-pause",
    "btn-reset",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveKeyMode {
    Arrows,
    Wasd,
}

impl MoveKeyMode {
    fn toggled(self) -> Self {
        match self {
            Self::Arrows => Self::Wasd,
            Self::Wasd => Self::Arrows,
        }
    }
}

#[derive(Default)]
struct InputState {
    left: bool,
    right: bool,
    down: bool,

    rotate_cw: bool,  // edge-triggered
    rotate_ccw: bool, // edge-triggered
    hard_drop: bool,  // edge-triggered
    hold: bool,       // edge-triggered
    pause: bool,      // edge-triggered
    reset: bool,      // edge-triggered

    horiz_dir: i32, // -1, 0, +1
    das: f64,
    arr: f64,
}

impl InputState {
    fn clear_edges(&mut self) {
        self.rotate_cw = false;
        self.rotate_ccw = false;
        self.hard_drop = false;
        self.hold = false;
        self.pause = false;
        self.reset = false;
    }
}

struct App {
    renderer: Renderer,
    game: Game,
    input: InputState,
    input_mode: MoveKeyMode,

    last_time: Option<f64>,
    overlay_hint: HtmlElement,
    entry_guidance: HtmlElement,
    guidance_lang_select: HtmlSelectElement,
    guidance_copy_zh: HtmlElement,
    guidance_copy_en: HtmlElement,
    guidance_start_btn: HtmlElement,
    input_mode_btn: HtmlElement,
    help_move_keys: HtmlElement,
    help_rotate_keys: HtmlElement,
    help_soft_drop_key: HtmlElement,
}

impl App {
    fn new(window: Window, document: Document) -> Result<Self, JsValue> {
        let renderer = Renderer::new(&document)?;
        renderer.resize_all()?;

        let seed = window
            .performance()
            .map(|p| p.now() as u32)
            .unwrap_or_else(|| js_sys::Date::now() as u32);

        let mut game = Game::new(seed);
        game.reset();

        let overlay_hint = document
            .get_element_by_id("overlay-hint")
            .ok_or_else(|| JsValue::from_str("Missing #overlay-hint"))?
            .dyn_into::<HtmlElement>()?;
        let entry_guidance = document
            .get_element_by_id("entry-guidance")
            .ok_or_else(|| JsValue::from_str("Missing #entry-guidance"))?
            .dyn_into::<HtmlElement>()?;
        let guidance_lang_select = document
            .get_element_by_id("guidance-lang")
            .ok_or_else(|| JsValue::from_str("Missing #guidance-lang"))?
            .dyn_into::<HtmlSelectElement>()?;
        let guidance_copy_zh = document
            .get_element_by_id("guidance-copy-zh")
            .ok_or_else(|| JsValue::from_str("Missing #guidance-copy-zh"))?
            .dyn_into::<HtmlElement>()?;
        let guidance_copy_en = document
            .get_element_by_id("guidance-copy-en")
            .ok_or_else(|| JsValue::from_str("Missing #guidance-copy-en"))?
            .dyn_into::<HtmlElement>()?;
        let guidance_start_btn = document
            .get_element_by_id("btn-guidance-close")
            .ok_or_else(|| JsValue::from_str("Missing #btn-guidance-close"))?
            .dyn_into::<HtmlElement>()?;

        let input_mode_btn = document
            .get_element_by_id("btn-input-mode")
            .ok_or_else(|| JsValue::from_str("Missing #btn-input-mode"))?
            .dyn_into::<HtmlElement>()?;
        let help_move_keys = document
            .get_element_by_id("help-move-keys")
            .ok_or_else(|| JsValue::from_str("Missing #help-move-keys"))?
            .dyn_into::<HtmlElement>()?;
        let help_rotate_keys = document
            .get_element_by_id("help-rotate-keys")
            .ok_or_else(|| JsValue::from_str("Missing #help-rotate-keys"))?
            .dyn_into::<HtmlElement>()?;
        let help_soft_drop_key = document
            .get_element_by_id("help-soft-drop-key")
            .ok_or_else(|| JsValue::from_str("Missing #help-soft-drop-key"))?
            .dyn_into::<HtmlElement>()?;

        let mut app = Self {
            renderer,
            game,
            input: InputState::default(),
            input_mode: MoveKeyMode::Arrows,
            last_time: None,
            overlay_hint,
            entry_guidance,
            guidance_lang_select,
            guidance_copy_zh,
            guidance_copy_en,
            guidance_start_btn,
            input_mode_btn,
            help_move_keys,
            help_rotate_keys,
            help_soft_drop_key,
        };
        app.sync_guidance_language_ui();
        app.sync_input_mode_ui();

        Ok(app)
    }

    fn hide_overlay_hint(&self) {
        let _ = self.overlay_hint.style().set_property("display", "none");
    }

    fn set_entry_guidance_visible(&self, visible: bool) {
        let display = if visible { "flex" } else { "none" };
        let _ = self.entry_guidance.style().set_property("display", display);
    }

    fn hide_entry_guidance(&self) {
        self.set_entry_guidance_visible(false);
    }

    fn show_entry_guidance(&self) {
        self.set_entry_guidance_visible(true);
    }

    fn is_entry_guidance_visible(&self) -> bool {
        self.entry_guidance
            .style()
            .get_property_value("display")
            .map(|v| v != "none")
            .unwrap_or(true)
    }

    fn sync_guidance_language_ui(&mut self) {
        let lang = self.guidance_lang_select.value();
        let is_en = lang == GUIDANCE_LANG_EN;

        let _ = self
            .guidance_copy_en
            .style()
            .set_property("display", if is_en { "block" } else { "none" });
        let _ = self
            .guidance_copy_zh
            .style()
            .set_property("display", if is_en { "none" } else { "block" });

        if is_en {
            self.guidance_start_btn.set_inner_text("Start Game");
            let _ = self
                .guidance_start_btn
                .set_attribute("aria-label", "Start game");
        } else {
            self.guidance_start_btn.set_inner_text("开始游戏");
            let _ = self
                .guidance_start_btn
                .set_attribute("aria-label", "开始游戏");
        }
    }

    fn is_left_key(&self, code: &str) -> bool {
        match self.input_mode {
            MoveKeyMode::Arrows => code == "ArrowLeft",
            MoveKeyMode::Wasd => code == "KeyA",
        }
    }

    fn is_right_key(&self, code: &str) -> bool {
        match self.input_mode {
            MoveKeyMode::Arrows => code == "ArrowRight",
            MoveKeyMode::Wasd => code == "KeyD",
        }
    }

    fn is_soft_drop_key(&self, code: &str) -> bool {
        match self.input_mode {
            MoveKeyMode::Arrows => code == "ArrowDown",
            MoveKeyMode::Wasd => code == "KeyS",
        }
    }

    fn is_rotate_cw_key(&self, code: &str) -> bool {
        match self.input_mode {
            MoveKeyMode::Arrows => matches!(code, "ArrowUp" | "KeyX"),
            MoveKeyMode::Wasd => matches!(code, "KeyW" | "KeyX"),
        }
    }

    fn sync_input_mode_ui(&mut self) {
        match self.input_mode {
            MoveKeyMode::Arrows => {
                self.input_mode_btn.set_inner_text("Input: Arrows");
                self.help_move_keys.set_inner_text("← / →");
                self.help_rotate_keys.set_inner_text("↑ (CW) • Z (CCW)");
                self.help_soft_drop_key.set_inner_text("↓");
            }
            MoveKeyMode::Wasd => {
                self.input_mode_btn.set_inner_text("Input: WASD");
                self.help_move_keys.set_inner_text("A / D");
                self.help_rotate_keys.set_inner_text("W (CW) • Z (CCW)");
                self.help_soft_drop_key.set_inner_text("S");
            }
        }
    }

    fn toggle_input_mode(&mut self) {
        self.input_mode = self.input_mode.toggled();

        // Avoid sticky movement when switching scheme while a key is held.
        self.input.left = false;
        self.input.right = false;
        self.input.down = false;
        self.input.horiz_dir = 0;
        self.input.das = 0.0;
        self.input.arr = 0.0;

        self.sync_input_mode_ui();
    }

    fn on_key_down(&mut self, e: KeyboardEvent) {
        let code = e.code();
        let code = code.as_str();

        // While entry guidance is open, only allow quick close keys.
        if self.is_entry_guidance_visible() {
            if matches!(code, "Enter" | "NumpadEnter" | "Escape") {
                e.prevent_default();
                self.hide_entry_guidance();
            }
            return;
        }

        // Prevent scrolling / page actions for keys we handle.
        let handled_key = self.is_left_key(code)
            || self.is_right_key(code)
            || self.is_soft_drop_key(code)
            || self.is_rotate_cw_key(code)
            || code == "Space";
        if handled_key {
            e.prevent_default();
        }

        self.hide_overlay_hint();

        if self.is_left_key(code) {
            self.input.left = true;
            return;
        }
        if self.is_right_key(code) {
            self.input.right = true;
            return;
        }
        if self.is_soft_drop_key(code) {
            self.input.down = true;
            return;
        }

        match code {
            // Rotations
            _ if self.is_rotate_cw_key(code) => {
                if !e.repeat() {
                    self.input.rotate_cw = true;
                }
            }
            "KeyZ" => {
                if !e.repeat() {
                    self.input.rotate_ccw = true;
                }
            }

            // Actions
            "Space" => {
                if !e.repeat() {
                    self.input.hard_drop = true;
                }
            }
            "KeyC" | "ShiftLeft" | "ShiftRight" => {
                if !e.repeat() {
                    self.input.hold = true;
                }
            }
            "KeyP" => {
                if !e.repeat() {
                    self.input.pause = true;
                }
            }
            "KeyR" => {
                if !e.repeat() {
                    self.input.reset = true;
                }
            }
            "KeyM" => {
                if !e.repeat() {
                    self.toggle_input_mode();
                }
            }
            _ => {}
        }
    }

    fn on_key_up(&mut self, e: KeyboardEvent) {
        let code = e.code();
        let code = code.as_str();

        if self.is_left_key(code) {
            self.input.left = false;
        } else if self.is_right_key(code) {
            self.input.right = false;
        } else if self.is_soft_drop_key(code) {
            self.input.down = false;
        }
    }

    fn on_button_down(&mut self, id: &str, e: PointerEvent) {
        // Keep the modal interaction strict so touches do not leak into gameplay.
        if self.is_entry_guidance_visible() {
            if id == "btn-guidance-close" {
                e.prevent_default();
                self.hide_entry_guidance();
            }
            return;
        }

        e.prevent_default();
        self.hide_overlay_hint();

        match id {
            "btn-left" => self.input.left = true,
            "btn-right" => self.input.right = true,
            "btn-down" => self.input.down = true,

            "btn-rot-cw" => self.input.rotate_cw = true,
            "btn-rot-ccw" => self.input.rotate_ccw = true,
            "btn-drop" => self.input.hard_drop = true,
            "btn-hold" => self.input.hold = true,

            "btn-pause" => self.input.pause = true,
            "btn-reset" => self.input.reset = true,
            "btn-input-mode" => self.toggle_input_mode(),
            "btn-open-guidance" => self.show_entry_guidance(),
            "btn-guidance-close" => {}

            _ => {}
        }
    }

    fn on_button_up(&mut self, id: &str, e: PointerEvent) {
        e.prevent_default();

        match id {
            "btn-left" => self.input.left = false,
            "btn-right" => self.input.right = false,
            "btn-down" => self.input.down = false,
            _ => {}
        }
    }

    fn handle_horizontal(&mut self, dt_ms: f64) {
        // Resolve direction.
        let dir = match (self.input.left, self.input.right) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        };

        if dir == 0 {
            self.input.horiz_dir = 0;
            self.input.das = 0.0;
            self.input.arr = 0.0;
            return;
        }

        // New press or direction change => move immediately and reset timers.
        if dir != self.input.horiz_dir {
            self.input.horiz_dir = dir;
            self.input.das = 0.0;
            self.input.arr = 0.0;
            self.game.move_horiz(dir);
            return;
        }

        // Held: DAS then ARR.
        self.input.das += dt_ms;
        if self.input.das < DAS_MS {
            return;
        }

        self.input.arr += dt_ms;
        while self.input.arr >= ARR_MS {
            self.input.arr -= ARR_MS;
            self.game.move_horiz(dir);
        }
    }

    fn frame(&mut self, time_ms: f64) -> Result<(), JsValue> {
        let dt = match self.last_time {
            None => {
                self.last_time = Some(time_ms);
                0.0
            }
            Some(last) => {
                self.last_time = Some(time_ms);
                (time_ms - last).clamp(0.0, 50.0) // clamp for tab-switch / background throttling
            }
        };

        // Edge-triggered UI actions
        if self.input.reset {
            self.game.reset();
        }
        if self.input.pause {
            self.game.toggle_pause();
        }

        // If paused/game-over, still render but suppress movement.
        if !self.game.is_paused() && !self.game.is_game_over() {
            self.handle_horizontal(dt);

            if self.input.rotate_cw {
                self.game.rotate(true);
            } else if self.input.rotate_ccw {
                self.game.rotate(false);
            }

            if self.input.hold {
                self.game.hold();
            }
            if self.input.hard_drop {
                self.game.hard_drop();
            }

            self.game.tick(dt, self.input.down);
        }

        self.renderer.render(&self.game)?;

        // Clear edge-triggered inputs after consuming them.
        self.input.clear_edges();

        Ok(())
    }
}

fn window() -> Result<Window, JsValue> {
    web_sys::window().ok_or_else(|| JsValue::from_str("No global window"))
}

fn document(window: &Window) -> Result<Document, JsValue> {
    window
        .document()
        .ok_or_else(|| JsValue::from_str("No document on window"))
}

fn bind_pointer_event(
    app: Rc<RefCell<App>>,
    el: &Element,
    id: &str,
    event_name: &str,
    is_down: bool,
) -> Result<(), JsValue> {
    // We intentionally keep one closure per event type and leak it with `forget`,
    // because wasm event listeners must stay alive for the page lifetime.
    let id = id.to_string();
    let cb = Closure::<dyn FnMut(PointerEvent)>::wrap(Box::new(move |e: PointerEvent| {
        if is_down {
            app.borrow_mut().on_button_down(&id, e);
        } else {
            app.borrow_mut().on_button_up(&id, e);
        }
    }));
    el.add_event_listener_with_callback(event_name, cb.as_ref().unchecked_ref())?;
    cb.forget();
    Ok(())
}

fn bind_button(app: Rc<RefCell<App>>, doc: &Document, id: &str) -> Result<(), JsValue> {
    let el = doc
        .get_element_by_id(id)
        .ok_or_else(|| JsValue::from_str(&format!("Missing #{id}")))?;

    bind_pointer_event(app.clone(), &el, id, "pointerdown", true)?;
    bind_pointer_event(app.clone(), &el, id, "pointerup", false)?;
    bind_pointer_event(app, &el, id, "pointercancel", false)?;

    Ok(())
}

fn bind_guidance_lang_select(
    app: Rc<RefCell<App>>,
    doc: &Document,
    id: &str,
) -> Result<(), JsValue> {
    let el = doc
        .get_element_by_id(id)
        .ok_or_else(|| JsValue::from_str(&format!("Missing #{id}")))?;

    let cb = Closure::<dyn FnMut(Event)>::wrap(Box::new(move |_e: Event| {
        app.borrow_mut().sync_guidance_language_ui();
    }));
    el.add_event_listener_with_callback("change", cb.as_ref().unchecked_ref())?;
    cb.forget();

    Ok(())
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let win = window()?;
    let doc = document(&win)?;

    let app = Rc::new(RefCell::new(App::new(win.clone(), doc.clone())?));

    // Keyboard listeners
    {
        let app = app.clone();
        let cb = Closure::<dyn FnMut(KeyboardEvent)>::wrap(Box::new(move |e: KeyboardEvent| {
            app.borrow_mut().on_key_down(e);
        }));
        win.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }
    {
        let app = app.clone();
        let cb = Closure::<dyn FnMut(KeyboardEvent)>::wrap(Box::new(move |e: KeyboardEvent| {
            app.borrow_mut().on_key_up(e);
        }));
        win.add_event_listener_with_callback("keyup", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    // Resize listener
    {
        let app = app.clone();
        let cb = Closure::<dyn FnMut()>::wrap(Box::new(move || {
            let _ = app.borrow().renderer.resize_all();
        }));
        win.add_event_listener_with_callback("resize", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    // Pointer controls (desktop + touch)
    for id in BUTTON_IDS {
        bind_button(app.clone(), &doc, id)?;
    }
    bind_guidance_lang_select(app.clone(), &doc, "guidance-lang")?;

    // RAF loop
    let f = Rc::new(RefCell::new(None::<Closure<dyn FnMut(f64)>>));
    let g = f.clone();

    let app_for_frame = app.clone();
    *g.borrow_mut() = Some(Closure::<dyn FnMut(f64)>::wrap(Box::new(move |t: f64| {
        {
            let _ = app_for_frame.borrow_mut().frame(t);
        }
        // schedule next frame
        let win = web_sys::window().unwrap();
        let cb = f.borrow();
        let cb = cb.as_ref().unwrap();
        let _ = win.request_animation_frame(cb.as_ref().unchecked_ref());
    })));

    // Kick off
    let win = web_sys::window().unwrap();
    let cb = g.borrow();
    let cb = cb.as_ref().unwrap();
    win.request_animation_frame(cb.as_ref().unchecked_ref())?;

    Ok(())
}

fn main() {}
