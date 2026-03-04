use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use web_sys::{CanvasRenderingContext2d, Document, HtmlCanvasElement, HtmlElement, Window};

use crate::game::{Game, PieceKind, HEIGHT, HIDDEN_ROWS, VISIBLE_HEIGHT, WIDTH};

const COLORS: [&str; 8] = [
    "rgba(0,0,0,0)", // empty
    "#00d8d8",       // I
    "#f4d61a",       // O
    "#a66bff",       // T
    "#22d06e",       // S
    "#ff4d5e",       // Z
    "#3b82ff",       // J
    "#ff9f1a",       // L
];
const NEXT_PREVIEW_COUNT: usize = 4;

pub struct Renderer {
    window: Window,

    board_canvas: HtmlCanvasElement,
    board_ctx: CanvasRenderingContext2d,
    board_w: f64,
    board_h: f64,

    side_hold_canvas: HtmlCanvasElement,
    side_hold_ctx: CanvasRenderingContext2d,
    side_hold_w: f64,
    side_hold_h: f64,

    side_next_canvas: HtmlCanvasElement,
    side_next_ctx: CanvasRenderingContext2d,
    side_next_w: f64,
    side_next_h: f64,

    score_el: HtmlElement,
    level_el: HtmlElement,
    lines_el: HtmlElement,
}

impl Renderer {
    pub fn new(doc: &Document) -> Result<Self, JsValue> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window"))?;

        let board_canvas = doc
            .get_element_by_id("board")
            .ok_or_else(|| JsValue::from_str("Missing #board"))?
            .dyn_into::<HtmlCanvasElement>()?;
        let side_hold_canvas = doc
            .get_element_by_id("hold-side")
            .ok_or_else(|| JsValue::from_str("Missing #hold-side"))?
            .dyn_into::<HtmlCanvasElement>()?;
        let side_next_canvas = doc
            .get_element_by_id("next-side")
            .ok_or_else(|| JsValue::from_str("Missing #next-side"))?
            .dyn_into::<HtmlCanvasElement>()?;

        let board_ctx = board_canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("No 2d context"))?
            .dyn_into::<CanvasRenderingContext2d>()?;
        let side_hold_ctx = side_hold_canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("No 2d context"))?
            .dyn_into::<CanvasRenderingContext2d>()?;
        let side_next_ctx = side_next_canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("No 2d context"))?
            .dyn_into::<CanvasRenderingContext2d>()?;

        let score_el = doc
            .get_element_by_id("score")
            .ok_or_else(|| JsValue::from_str("Missing #score"))?
            .dyn_into::<HtmlElement>()?;
        let level_el = doc
            .get_element_by_id("level")
            .ok_or_else(|| JsValue::from_str("Missing #level"))?
            .dyn_into::<HtmlElement>()?;
        let lines_el = doc
            .get_element_by_id("lines")
            .ok_or_else(|| JsValue::from_str("Missing #lines"))?
            .dyn_into::<HtmlElement>()?;

        Ok(Self {
            window,

            board_canvas,
            board_ctx,
            board_w: 0.0,
            board_h: 0.0,

            side_hold_canvas,
            side_hold_ctx,
            side_hold_w: 0.0,
            side_hold_h: 0.0,

            side_next_canvas,
            side_next_ctx,
            side_next_w: 0.0,
            side_next_h: 0.0,

            score_el,
            level_el,
            lines_el,
        })
    }

    pub fn resize_all(&self) -> Result<(), JsValue> {
        self.sync_touch_controls_height();

        // This method is called through shared reference, but we only mutate DOM state.
        // It's fine as all DOM operations are interior-mutable.
        resize_canvas(&self.window, &self.board_canvas, &self.board_ctx)?;
        resize_canvas(&self.window, &self.side_hold_canvas, &self.side_hold_ctx)?;
        resize_canvas(&self.window, &self.side_next_canvas, &self.side_next_ctx)?;
        Ok(())
    }

    fn sync_touch_controls_height(&self) {
        let Some(doc) = self.window.document() else {
            return;
        };
        let Some(app_el) = doc.get_element_by_id("app") else {
            return;
        };
        let Some(app) = app_el.dyn_ref::<HtmlElement>() else {
            return;
        };

        // The controls are fixed-position; expose their measured height as a CSS var so
        // board sizing and page bottom padding can avoid overlap on all touch layouts.
        let controls_h = doc
            .get_element_by_id("touch-controls")
            .and_then(|el| el.dyn_into::<HtmlElement>().ok())
            .map(|el| el.offset_height())
            .filter(|h| *h > 0)
            .unwrap_or(0);

        let _ = app
            .style()
            .set_property("--touch-controls-height", &format!("{controls_h}px"));
    }

    pub fn render(&mut self, game: &Game) -> Result<(), JsValue> {
        // Cache CSS sizes (cheap, but avoid repeating during draw calls).
        self.board_w = self.board_canvas.client_width() as f64;
        self.board_h = self.board_canvas.client_height() as f64;
        self.side_hold_w = self.side_hold_canvas.client_width() as f64;
        self.side_hold_h = self.side_hold_canvas.client_height() as f64;
        self.side_next_w = self.side_next_canvas.client_width() as f64;
        self.side_next_h = self.side_next_canvas.client_height() as f64;

        self.score_el.set_inner_text(&game.score.to_string());
        self.level_el.set_inner_text(&game.level.to_string());
        self.lines_el.set_inner_text(&game.lines.to_string());

        self.draw_board(game)?;
        self.draw_side_hold(game)?;
        self.draw_side_next(game)?;

        Ok(())
    }

    fn draw_board(&mut self, game: &Game) -> Result<(), JsValue> {
        let ctx = &self.board_ctx;
        let w = self.board_w;
        let h = self.board_h;

        // Background
        ctx.set_fill_style_str("#0f1521");
        ctx.fill_rect(0.0, 0.0, w, h);

        // Grid cell size (10×20 visible).
        let cell = w / (WIDTH as f64);

        // Subtle grid
        ctx.set_stroke_style_str("rgba(255,255,255,0.06)");
        ctx.set_line_width(1.0);
        ctx.begin_path();
        for i in 1..WIDTH {
            let x = (i as f64) * cell + 0.5;
            ctx.move_to(x, 0.0);
            ctx.line_to(x, h);
        }
        for j in 1..=VISIBLE_HEIGHT {
            let y = (j as f64) * cell + 0.5;
            ctx.move_to(0.0, y);
            ctx.line_to(w, y);
        }
        ctx.stroke();

        // Placed blocks (visible rows only)
        for y in HIDDEN_ROWS..HEIGHT {
            let vis_y = (y - HIDDEN_ROWS) as f64;
            for x in 0..WIDTH {
                let idx = y * WIDTH + x;
                let v = game.board[idx] as usize;
                if v == 0 {
                    continue;
                }
                draw_mino(ctx, (x as f64) * cell, vis_y * cell, cell, COLORS[v], 1.0)?;
            }
        }

        // Ghost
        let ghost = game.ghost();
        for (x, y) in ghost.cells() {
            if y < HIDDEN_ROWS as i32 {
                continue;
            }
            let vy = (y as usize - HIDDEN_ROWS) as f64;
            draw_mino(
                ctx,
                (x as f64) * cell,
                vy * cell,
                cell,
                COLORS[ghost.kind.color_index() as usize],
                0.18,
            )?;
        }

        // Current piece
        for (x, y) in game.current.cells() {
            if y < HIDDEN_ROWS as i32 {
                continue;
            }
            let vy = (y as usize - HIDDEN_ROWS) as f64;
            draw_mino(
                ctx,
                (x as f64) * cell,
                vy * cell,
                cell,
                COLORS[game.current.kind.color_index() as usize],
                1.0,
            )?;
        }

        // Overlay states
        if game.is_paused() || game.is_game_over() {
            ctx.set_fill_style_str("rgba(0,0,0,0.55)");
            ctx.fill_rect(0.0, 0.0, w, h);

            ctx.set_fill_style_str("rgba(231,235,242,0.92)");
            ctx.set_font("700 26px system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial");
            ctx.set_text_align("center");
            ctx.set_text_baseline("middle");

            let msg = if game.is_game_over() {
                "GAME OVER"
            } else {
                "PAUSED"
            };
            ctx.fill_text(msg, w / 2.0, h / 2.0)?;
            if game.is_game_over() {
                ctx.set_font(
                    "600 14px system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial",
                );
                ctx.set_fill_style_str("rgba(231,235,242,0.76)");
                ctx.fill_text(
                    "Press R (or Restart) to play again",
                    w / 2.0,
                    h / 2.0 + 34.0,
                )?;
            }
        }

        Ok(())
    }

    fn draw_side_hold(&mut self, game: &Game) -> Result<(), JsValue> {
        let ctx = &self.side_hold_ctx;
        let w = self.side_hold_w;
        let h = self.side_hold_h;

        ctx.set_fill_style_str("#121a29");
        ctx.fill_rect(0.0, 0.0, w, h);

        if let Some(kind) = game.hold {
            draw_piece_in_box(ctx, kind, w, h, 1.0)?;
        } else {
            ctx.set_fill_style_str("rgba(255,255,255,0.18)");
            ctx.set_font("700 11px system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial");
            ctx.set_text_align("center");
            ctx.set_text_baseline("middle");
            ctx.fill_text("H", w / 2.0, h / 2.0)?;
        }

        Ok(())
    }

    fn draw_side_next(&mut self, game: &Game) -> Result<(), JsValue> {
        let ctx = &self.side_next_ctx;
        let w = self.side_next_w;
        let h = self.side_next_h;

        ctx.set_fill_style_str("#121a29");
        ctx.fill_rect(0.0, 0.0, w, h);

        // Draw a compact vertical stack of upcoming pieces.
        let slot_h = h / (NEXT_PREVIEW_COUNT as f64);
        for (i, kind) in game.next.iter().take(NEXT_PREVIEW_COUNT).enumerate() {
            let y0 = (i as f64) * slot_h;
            draw_piece_in_slot(ctx, *kind, w, slot_h, y0)?;
        }

        Ok(())
    }
}

fn resize_canvas(
    win: &Window,
    canvas: &HtmlCanvasElement,
    ctx: &CanvasRenderingContext2d,
) -> Result<(), JsValue> {
    let dpr = win.device_pixel_ratio();
    let css_w = canvas.client_width() as f64;
    let css_h = canvas.client_height() as f64;

    if css_w <= 0.0 || css_h <= 0.0 {
        return Ok(());
    }

    canvas.set_width((css_w * dpr) as u32);
    canvas.set_height((css_h * dpr) as u32);

    // Draw using CSS pixels.
    ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0)?;
    Ok(())
}

fn draw_mino(
    ctx: &CanvasRenderingContext2d,
    x: f64,
    y: f64,
    cell: f64,
    color: &str,
    alpha: f64,
) -> Result<(), JsValue> {
    let pad = cell * 0.08;
    let w = cell - pad * 2.0;

    ctx.save();
    ctx.set_global_alpha(alpha);

    // Base
    ctx.set_fill_style_str(color);
    ctx.fill_rect(x + pad, y + pad, w, w);

    // Highlight
    ctx.set_fill_style_str("rgba(255,255,255,0.20)");
    ctx.fill_rect(x + pad, y + pad, w, w * 0.22);
    ctx.fill_rect(x + pad, y + pad, w * 0.22, w);

    // Shadow
    ctx.set_fill_style_str("rgba(0,0,0,0.22)");
    ctx.fill_rect(x + pad, y + pad + w * 0.78, w, w * 0.22);
    ctx.fill_rect(x + pad + w * 0.78, y + pad, w * 0.22, w);

    // Inner stroke
    ctx.set_stroke_style_str("rgba(0,0,0,0.20)");
    ctx.set_line_width(1.0);
    ctx.stroke_rect(x + pad + 0.5, y + pad + 0.5, w - 1.0, w - 1.0);

    ctx.restore();
    Ok(())
}

fn draw_piece_in_box(
    ctx: &CanvasRenderingContext2d,
    kind: PieceKind,
    w: f64,
    h: f64,
    alpha: f64,
) -> Result<(), JsValue> {
    // 4×4 cells centered.
    let cell = (w.min(h)) / 4.8;
    let ox = (w - cell * 4.0) / 2.0;
    let oy = (h - cell * 4.0) / 2.0;

    let (cells, color) = piece_preview(kind);
    for (x, y) in cells {
        draw_mino(
            ctx,
            ox + (x as f64) * cell,
            oy + (y as f64) * cell,
            cell,
            color,
            alpha,
        )?;
    }
    Ok(())
}

fn draw_piece_in_slot(
    ctx: &CanvasRenderingContext2d,
    kind: PieceKind,
    w: f64,
    slot_h: f64,
    y0: f64,
) -> Result<(), JsValue> {
    // 4×4 in a slot, slightly smaller to avoid touching borders.
    let cell = (w.min(slot_h)) / 5.0;
    let ox = (w - cell * 4.0) / 2.0;
    let oy = y0 + (slot_h - cell * 4.0) / 2.0;

    let (cells, color) = piece_preview(kind);
    for (x, y) in cells {
        draw_mino(
            ctx,
            ox + (x as f64) * cell,
            oy + (y as f64) * cell,
            cell,
            color,
            1.0,
        )?;
    }
    Ok(())
}

// A compact 4×4 preview for each piece (rot 0), tuned for nice centering.
fn piece_preview(kind: PieceKind) -> ([(i32, i32); 4], &'static str) {
    let color = COLORS[kind.color_index() as usize];
    let cells = match kind {
        PieceKind::I => [(0, 1), (1, 1), (2, 1), (3, 1)],
        PieceKind::O => [(1, 1), (2, 1), (1, 2), (2, 2)],
        PieceKind::T => [(1, 1), (0, 2), (1, 2), (2, 2)],
        PieceKind::S => [(1, 1), (2, 1), (0, 2), (1, 2)],
        PieceKind::Z => [(0, 1), (1, 1), (1, 2), (2, 2)],
        PieceKind::J => [(0, 1), (0, 2), (1, 2), (2, 2)],
        PieceKind::L => [(2, 1), (0, 2), (1, 2), (2, 2)],
    };
    (cells, color)
}
