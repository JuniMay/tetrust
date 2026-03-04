use std::collections::VecDeque;

pub const WIDTH: usize = 10;
pub const HEIGHT: usize = 22; // includes hidden spawn rows
pub const HIDDEN_ROWS: usize = 2;
pub const VISIBLE_HEIGHT: usize = 20;

const BOARD_LEN: usize = WIDTH * HEIGHT;

const LOCK_DELAY_MS: f64 = 500.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceKind {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}

impl PieceKind {
    pub const ALL: [PieceKind; 7] = [
        PieceKind::I,
        PieceKind::O,
        PieceKind::T,
        PieceKind::S,
        PieceKind::Z,
        PieceKind::J,
        PieceKind::L,
    ];

    pub fn color_index(self) -> u8 {
        match self {
            PieceKind::I => 1,
            PieceKind::O => 2,
            PieceKind::T => 3,
            PieceKind::S => 4,
            PieceKind::Z => 5,
            PieceKind::J => 6,
            PieceKind::L => 7,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ActivePiece {
    pub kind: PieceKind,
    pub rot: u8, // 0..=3
    pub x: i32,
    pub y: i32,
}

impl ActivePiece {
    pub fn cells(self) -> [(i32, i32); 4] {
        let shape = shape_cells(self.kind, self.rot);
        [
            (self.x + shape[0].0, self.y + shape[0].1),
            (self.x + shape[1].0, self.y + shape[1].1),
            (self.x + shape[2].0, self.y + shape[2].1),
            (self.x + shape[3].0, self.y + shape[3].1),
        ]
    }
}

#[derive(Clone)]
pub struct Game {
    pub board: [u8; BOARD_LEN],

    pub current: ActivePiece,
    pub hold: Option<PieceKind>,
    pub hold_used: bool,
    pub next: VecDeque<PieceKind>, // preview queue

    pub score: u64,
    pub lines: u32,
    pub level: u32,

    paused: bool,
    game_over: bool,

    // RNG / bag
    rng: XorShift32,
    bag: Vec<PieceKind>,

    // Timers
    gravity_acc: f64,
    lock_acc: f64,
}

impl Game {
    pub fn new(seed: u32) -> Self {
        let rng = XorShift32::new(seed);
        let next = VecDeque::new();

        let mut g = Self {
            board: [0; BOARD_LEN],
            current: ActivePiece {
                kind: PieceKind::T,
                rot: 0,
                x: 3,
                y: 0,
            },
            hold: None,
            hold_used: false,
            next,
            score: 0,
            lines: 0,
            level: 1,
            paused: false,
            game_over: false,
            rng,
            bag: Vec::new(),
            gravity_acc: 0.0,
            lock_acc: 0.0,
        };

        g.refill_next_queue();
        g
    }

    pub fn reset(&mut self) {
        self.board = [0; BOARD_LEN];
        self.score = 0;
        self.lines = 0;
        self.level = 1;
        self.paused = false;
        self.game_over = false;

        self.hold = None;
        self.hold_used = false;

        self.bag.clear();
        self.next.clear();
        self.refill_next_queue();

        self.gravity_acc = 0.0;
        self.lock_acc = 0.0;

        self.spawn_next();
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn is_game_over(&self) -> bool {
        self.game_over
    }

    pub fn toggle_pause(&mut self) {
        if self.game_over {
            return;
        }
        self.paused = !self.paused;
    }

    pub fn tick(&mut self, dt_ms: f64, soft_drop: bool) {
        if self.paused || self.game_over {
            return;
        }

        let gravity_ms = if soft_drop {
            50.0
        } else {
            gravity_interval_ms(self.level)
        };

        self.gravity_acc += dt_ms;

        while self.gravity_acc >= gravity_ms {
            self.gravity_acc -= gravity_ms;

            if self.try_move(0, 1) {
                if soft_drop {
                    self.score = self.score.saturating_add(1);
                }
            } else {
                // Once grounded, further gravity doesn't apply.
                self.gravity_acc = 0.0;
                break;
            }
        }

        if self.can_move_down() {
            self.lock_acc = 0.0;
        } else {
            self.lock_acc += dt_ms;
            if self.lock_acc >= LOCK_DELAY_MS {
                self.lock_piece();
            }
        }
    }

    pub fn move_horiz(&mut self, dx: i32) -> bool {
        if self.paused || self.game_over {
            return false;
        }
        if self.try_move(dx, 0) {
            self.lock_acc = 0.0;
            true
        } else {
            false
        }
    }

    pub fn rotate(&mut self, clockwise: bool) -> bool {
        if self.paused || self.game_over {
            return false;
        }

        let from = self.current.rot;
        let to = if clockwise {
            (from + 1) & 3
        } else {
            (from + 3) & 3
        };

        // O piece doesn't need kicks (rotation is effectively a no-op visually).
        if self.current.kind == PieceKind::O {
            self.current.rot = to;
            self.lock_acc = 0.0;
            return true;
        }

        let kicks = srs_kicks(self.current.kind, from, to);

        for (dx, dy) in kicks {
            let candidate = ActivePiece {
                kind: self.current.kind,
                rot: to,
                x: self.current.x + dx,
                y: self.current.y + dy,
            };
            if self.can_place(&candidate) {
                self.current = candidate;
                self.lock_acc = 0.0;
                return true;
            }
        }

        false
    }

    pub fn hard_drop(&mut self) {
        if self.paused || self.game_over {
            return;
        }

        let mut dist = 0u32;
        while self.try_move(0, 1) {
            dist += 1;
        }
        self.score = self.score.saturating_add((dist as u64) * 2);
        self.lock_piece();
    }

    pub fn hold(&mut self) {
        if self.paused || self.game_over {
            return;
        }
        if self.hold_used {
            return;
        }
        self.hold_used = true;

        let current_kind = self.current.kind;
        match self.hold {
            None => {
                self.hold = Some(current_kind);
                self.spawn_next();
            }
            Some(hold_kind) => {
                self.hold = Some(current_kind);
                self.current = ActivePiece {
                    kind: hold_kind,
                    rot: 0,
                    x: 3,
                    y: 0,
                };
                if !self.can_place(&self.current) {
                    self.game_over = true;
                }
            }
        }

        self.gravity_acc = 0.0;
        self.lock_acc = 0.0;
    }

    pub fn ghost(&self) -> ActivePiece {
        let mut p = self.current;
        while self.can_place(&ActivePiece { y: p.y + 1, ..p }) {
            p.y += 1;
        }
        p
    }

    fn refill_next_queue(&mut self) {
        while self.next.len() < 5 {
            let piece = self.next_from_bag();
            self.next.push_back(piece);
        }
    }

    fn spawn_next(&mut self) {
        self.refill_next_queue();
        let kind = self.next.pop_front().unwrap();
        let piece = self.next_from_bag();
        self.next.push_back(piece);

        self.current = ActivePiece {
            kind,
            rot: 0,
            x: 3,
            y: 0,
        };

        self.hold_used = false;

        if !self.can_place(&self.current) {
            self.game_over = true;
        }
    }

    fn lock_piece(&mut self) {
        // Write piece blocks to board.
        let color = self.current.kind.color_index();
        for (x, y) in self.current.cells() {
            if x < 0 || x >= WIDTH as i32 || y < 0 || y >= HEIGHT as i32 {
                continue;
            }
            let idx = (y as usize) * WIDTH + (x as usize);
            self.board[idx] = color;
        }

        let cleared = self.clear_lines();
        if cleared > 0 {
            self.lines += cleared;

            let base = match cleared {
                1 => 100,
                2 => 300,
                3 => 500,
                4 => 800,
                _ => 0,
            };

            self.score = self
                .score
                .saturating_add((base as u64) * (self.level as u64));
            self.level = 1 + (self.lines / 10);
        }

        self.gravity_acc = 0.0;
        self.lock_acc = 0.0;

        self.spawn_next();
    }

    fn clear_lines(&mut self) -> u32 {
        let mut cleared = 0u32;
        let mut y = (HEIGHT as i32) - 1;

        while y >= 0 {
            if self.row_full(y as usize) {
                cleared += 1;

                // Shift rows down.
                for yy in (1..=y as usize).rev() {
                    for x in 0..WIDTH {
                        self.board[yy * WIDTH + x] = self.board[(yy - 1) * WIDTH + x];
                    }
                }
                // Clear top row.
                for x in 0..WIDTH {
                    self.board[x] = 0;
                }

                // Re-check same y (now contains row above).
            } else {
                y -= 1;
            }
        }

        cleared
    }

    fn row_full(&self, y: usize) -> bool {
        let row = &self.board[y * WIDTH..(y + 1) * WIDTH];
        row.iter().all(|&c| c != 0)
    }

    fn can_move_down(&self) -> bool {
        let mut p = self.current;
        p.y += 1;
        self.can_place(&p)
    }

    fn try_move(&mut self, dx: i32, dy: i32) -> bool {
        let candidate = ActivePiece {
            x: self.current.x + dx,
            y: self.current.y + dy,
            ..self.current
        };
        if self.can_place(&candidate) {
            self.current = candidate;
            true
        } else {
            false
        }
    }

    fn can_place(&self, piece: &ActivePiece) -> bool {
        for (x, y) in piece.cells() {
            if x < 0 || x >= WIDTH as i32 || y < 0 || y >= HEIGHT as i32 {
                return false;
            }
            let idx = (y as usize) * WIDTH + (x as usize);
            if self.board[idx] != 0 {
                return false;
            }
        }
        true
    }

    fn next_from_bag(&mut self) -> PieceKind {
        if self.bag.is_empty() {
            self.bag = PieceKind::ALL.to_vec();
            self.rng.shuffle(&mut self.bag);
        }
        self.bag.pop().unwrap()
    }
}

// ---- RNG ----

#[derive(Clone)]
struct XorShift32(u32);

impl XorShift32 {
    fn new(seed: u32) -> Self {
        let seed = if seed == 0 { 0x6D2B_79F5 } else { seed };
        Self(seed)
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    fn next_usize(&mut self, max: usize) -> usize {
        (self.next_u32() as usize) % max
    }

    fn shuffle<T>(&mut self, slice: &mut [T]) {
        for i in (1..slice.len()).rev() {
            let j = self.next_usize(i + 1);
            slice.swap(i, j);
        }
    }
}

// ---- Shapes ----
// Coordinates are within a 4×4 box, using (x,y) with y increasing downward.

fn shape_cells(kind: PieceKind, rot: u8) -> [(i32, i32); 4] {
    match kind {
        PieceKind::I => match rot & 3 {
            0 => [(0, 1), (1, 1), (2, 1), (3, 1)],
            1 => [(2, 0), (2, 1), (2, 2), (2, 3)],
            2 => [(0, 2), (1, 2), (2, 2), (3, 2)],
            _ => [(1, 0), (1, 1), (1, 2), (1, 3)],
        },
        PieceKind::O => [(1, 0), (2, 0), (1, 1), (2, 1)],
        PieceKind::T => match rot & 3 {
            0 => [(1, 0), (0, 1), (1, 1), (2, 1)],
            1 => [(1, 0), (1, 1), (2, 1), (1, 2)],
            2 => [(0, 1), (1, 1), (2, 1), (1, 2)],
            _ => [(1, 0), (0, 1), (1, 1), (1, 2)],
        },
        PieceKind::S => match rot & 3 {
            0 => [(1, 0), (2, 0), (0, 1), (1, 1)],
            1 => [(1, 0), (1, 1), (2, 1), (2, 2)],
            2 => [(1, 1), (2, 1), (0, 2), (1, 2)],
            _ => [(0, 0), (0, 1), (1, 1), (1, 2)],
        },
        PieceKind::Z => match rot & 3 {
            0 => [(0, 0), (1, 0), (1, 1), (2, 1)],
            1 => [(2, 0), (1, 1), (2, 1), (1, 2)],
            2 => [(0, 1), (1, 1), (1, 2), (2, 2)],
            _ => [(1, 0), (0, 1), (1, 1), (0, 2)],
        },
        PieceKind::J => match rot & 3 {
            0 => [(0, 0), (0, 1), (1, 1), (2, 1)],
            1 => [(1, 0), (2, 0), (1, 1), (1, 2)],
            2 => [(0, 1), (1, 1), (2, 1), (2, 2)],
            _ => [(1, 0), (1, 1), (0, 2), (1, 2)],
        },
        PieceKind::L => match rot & 3 {
            0 => [(2, 0), (0, 1), (1, 1), (2, 1)],
            1 => [(1, 0), (1, 1), (1, 2), (2, 2)],
            2 => [(0, 1), (1, 1), (2, 1), (0, 2)],
            _ => [(0, 0), (1, 0), (1, 1), (1, 2)],
        },
    }
}

// ---- SRS kicks ----
// Kick data is taken from standard SRS tables (Hard Drop), but converted to y-down coordinates.

fn srs_kicks(kind: PieceKind, from: u8, to: u8) -> &'static [(i32, i32)] {
    match kind {
        PieceKind::I => srs_kicks_i(from, to),
        PieceKind::O => &[(0, 0)],
        _ => srs_kicks_jlstz(from, to),
    }
}

fn srs_kicks_jlstz(from: u8, to: u8) -> &'static [(i32, i32)] {
    match (from & 3, to & 3) {
        (0, 1) => &[(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
        (1, 0) => &[(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],
        (1, 2) => &[(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],
        (2, 1) => &[(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
        (2, 3) => &[(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
        (3, 2) => &[(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],
        (3, 0) => &[(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],
        (0, 3) => &[(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
        _ => &[(0, 0)],
    }
}

fn srs_kicks_i(from: u8, to: u8) -> &'static [(i32, i32)] {
    match (from & 3, to & 3) {
        (0, 1) => &[(0, 0), (-2, 0), (1, 0), (-2, 1), (1, -2)],
        (1, 0) => &[(0, 0), (2, 0), (-1, 0), (2, -1), (-1, 2)],
        (1, 2) => &[(0, 0), (-1, 0), (2, 0), (-1, -2), (2, 1)],
        (2, 1) => &[(0, 0), (1, 0), (-2, 0), (1, 2), (-2, -1)],
        (2, 3) => &[(0, 0), (2, 0), (-1, 0), (2, -1), (-1, 2)],
        (3, 2) => &[(0, 0), (-2, 0), (1, 0), (-2, 1), (1, -2)],
        (3, 0) => &[(0, 0), (1, 0), (-2, 0), (1, 2), (-2, -1)],
        (0, 3) => &[(0, 0), (-1, 0), (2, 0), (-1, -2), (2, 1)],
        _ => &[(0, 0)],
    }
}

// ---- Timing ----

fn gravity_interval_ms(level: u32) -> f64 {
    // Smooth exponential curve with a hard cap.
    // Level 1 ≈ 800ms, Level 10 ≈ 157ms, caps at 50ms.
    let lvl = level.max(1) as i32;
    let base = 800.0 * 0.85_f64.powi(lvl - 1);
    base.max(50.0)
}
