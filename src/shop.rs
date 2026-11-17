//! The shop menu. What a shop sells is described once, as tabs of rows, and
//! that one description feeds both the drawing and the input, so a row can
//! never be clickable somewhere it isn't drawn. It lives apart from the
//! renderer because it is the one piece of the screen the player operates.

use macroquad::prelude::*;

use crate::game::{BASE_KIT_PRICE, Game, Gun, MAX_BASE_KITS, ShopAction, SiteKind, Upgrade};
use crate::render::{GOLD, PANEL, money, text, text_centered};

/// How wide the panel's rows are, and so how much of the screen's right
/// edge the shop takes.
pub const WIDTH: f32 = 360.0;
const MARGIN: f32 = 16.0;
const PAD: f32 = 12.0;
const TOP: f32 = 14.0;
const TAB_H: f32 = 32.0;
const ROW_H: f32 = 40.0;
const GAP: f32 = 6.0;

/// The keys that pick a row, top to bottom. NOTE: no tab may hold more rows
/// than this, or its last rows can only be clicked.
const KEYS: [(KeyCode, char); 10] = [
    (KeyCode::Key1, '1'),
    (KeyCode::Key2, '2'),
    (KeyCode::Key3, '3'),
    (KeyCode::Key4, '4'),
    (KeyCode::Key5, '5'),
    (KeyCode::Key6, '6'),
    (KeyCode::Key7, '7'),
    (KeyCode::Key8, '8'),
    (KeyCode::Key9, '9'),
    (KeyCode::Key0, '0'),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Tab {
    /// Selling, fuel and repairs: what every visit starts with.
    #[default]
    Service,
    Upgrades,
    /// Guns and base kits.
    Gear,
}

impl Tab {
    const ALL: [Tab; 3] = [Tab::Service, Tab::Upgrades, Tab::Gear];

    fn title(self) -> &'static str {
        match self {
            Tab::Service => "Service",
            Tab::Upgrades => "Upgrades",
            Tab::Gear => "Gear",
        }
    }

    fn next(self) -> Tab {
        match self {
            Tab::Service => Tab::Upgrades,
            Tab::Upgrades => Tab::Gear,
            Tab::Gear => Tab::Service,
        }
    }
}

/// One thing the shop offers.
struct Row {
    action: ShopAction,
    label: String,
    /// The price, or why there isn't one.
    detail: String,
    enabled: bool,
}

impl Row {
    fn new(action: ShopAction, label: impl Into<String>, detail: String, enabled: bool) -> Row {
        Row {
            action,
            label: label.into(),
            detail,
            enabled,
        }
    }

    /// Something bought once at a fixed price, or already had.
    fn purchase(
        action: ShopAction,
        label: String,
        price: u64,
        sold_out: Option<&str>,
        game: &Game,
    ) -> Row {
        match sold_out {
            Some(why) => Row::new(action, label, why.into(), false),
            None => Row::new(action, label, money(price), game.money >= price),
        }
    }
}

fn rows(game: &Game, tab: Tab) -> Vec<Row> {
    match tab {
        Tab::Service => {
            let (gems, minerals) = (game.cargo_value(), game.mineral_value());
            let (fuel, repair) = (game.refuel_cost(), game.repair_cost());
            vec![
                Row::new(
                    ShopAction::Sell,
                    "Sell gems and meat",
                    format!("+{}", money(gems)),
                    gems > 0,
                ),
                Row::new(
                    ShopAction::SellMinerals,
                    "Sell spare minerals",
                    format!("+{}", money(minerals)),
                    minerals > 0,
                ),
                Row::new(
                    ShopAction::Refuel,
                    "Refuel",
                    money(fuel),
                    fuel > 0 && game.money > 0,
                ),
                Row::new(
                    ShopAction::Repair,
                    "Repair hull",
                    money(repair),
                    repair > 0 && game.money > 0,
                ),
            ]
        }
        Tab::Upgrades => Upgrade::ALL
            .iter()
            .map(|&u| {
                let level = game.levels.get(u);
                Row::purchase(
                    ShopAction::Buy(u),
                    format!("{}  Lv {level}/{}", u.name(), u.max_level()),
                    u.cost(level),
                    (level >= u.max_level()).then_some("MAX"),
                    game,
                )
            })
            .collect(),
        Tab::Gear => {
            let guns = Gun::ALL.iter().map(|&g| {
                Row::purchase(
                    ShopAction::BuyGun(g),
                    g.name().into(),
                    g.price(),
                    game.arsenal.has(g).then_some("OWNED"),
                    game,
                )
            });
            let kit = Row::purchase(
                ShopAction::BuyBaseKit,
                format!("Base kit  ({}/{MAX_BASE_KITS})", game.base_kits),
                BASE_KIT_PRICE,
                (game.base_kits >= MAX_BASE_KITS).then_some("FULL"),
                game,
            );
            guns.chain([kit]).collect()
        }
    }
}

/// Where the open tab's pieces sit on a screen `sw` wide.
struct Layout {
    panel: Rect,
    tabs: [(Rect, Tab); 3],
    rows: Vec<(Rect, Row)>,
    /// Baseline of the line of help under the rows.
    hint: f32,
}

fn layout(game: &Game, tab: Tab, sw: f32) -> Layout {
    let x = sw - WIDTH - MARGIN - PAD;
    let tabs_y = TOP + 44.0;
    let tab_w = (WIDTH - 2.0 * GAP) / 3.0;
    let tabs = Tab::ALL.map(|t| {
        let i = Tab::ALL.iter().position(|&a| a == t).unwrap_or(0) as f32;
        (Rect::new(x + i * (tab_w + GAP), tabs_y, tab_w, TAB_H), t)
    });
    let mut y = tabs_y + TAB_H + GAP + 4.0;
    let mut placed = Vec::new();
    for row in rows(game, tab) {
        placed.push((Rect::new(x, y, WIDTH, ROW_H), row));
        y += ROW_H + GAP;
    }
    let hint = y + 16.0;
    Layout {
        panel: Rect::new(x - PAD, TOP, WIDTH + 2.0 * PAD, hint + PAD - TOP),
        tabs,
        rows: placed,
        hint,
    }
}

/// The shop's menu: which tab is open.
#[derive(Default)]
pub struct ShopMenu {
    tab: Tab,
}

impl ShopMenu {
    /// Takes this frame's keys and clicks. Away from a shop it does nothing
    /// but turn back to the first tab, so every visit opens on selling.
    pub fn input(&mut self, game: &mut Game) {
        if !game.at_shop() {
            self.tab = Tab::default();
            return;
        }
        if is_key_pressed(KeyCode::Tab) {
            self.tab = self.tab.next();
        }
        let click =
            is_mouse_button_pressed(MouseButton::Left).then(|| Vec2::from(mouse_position()));
        let lay = layout(game, self.tab, screen_width());
        if let Some(&(_, tab)) = lay
            .tabs
            .iter()
            .find(|(r, _)| click.is_some_and(|m| r.contains(m)))
        {
            self.tab = tab;
            return;
        }
        for ((rect, row), (key, _)) in lay.rows.iter().zip(KEYS) {
            if is_key_pressed(key) || click.is_some_and(|m| rect.contains(m)) {
                game.apply(row.action);
            }
        }
    }

    pub fn draw(&self, game: &Game, sw: f32) {
        let lay = layout(game, self.tab, sw);
        let p = lay.panel;
        draw_rectangle(p.x, p.y, p.w, p.h, PANEL);
        let title = match game.current_site().map(|s| s.kind) {
            Some(SiteKind::Outpost) => "OUTPOST",
            Some(SiteKind::Base) => "YOUR BASE",
            _ => "SHOP",
        };
        text(title, p.x + PAD, TOP + 32.0, 26.0, GOLD);
        let mouse = Vec2::from(mouse_position());
        let dim = Color::new(0.5, 0.5, 0.55, 1.0);
        for (r, tab) in lay.tabs {
            let open = tab == self.tab;
            let bg = match (open, r.contains(mouse)) {
                (true, _) => Color::new(0.35, 0.3, 0.15, 1.0),
                (false, true) => Color::new(0.2, 0.18, 0.12, 1.0),
                _ => Color::new(0.12, 0.12, 0.14, 1.0),
            };
            draw_rectangle(r.x, r.y, r.w, r.h, bg);
            let fg = if open { GOLD } else { dim };
            text_centered(tab.title(), r.x + r.w / 2.0, r.y + 22.0, 20.0, fg);
        }
        for ((r, row), (_, digit)) in lay.rows.iter().zip(KEYS) {
            let bg = match (row.enabled, r.contains(mouse)) {
                (true, true) => Color::new(0.35, 0.3, 0.15, 1.0),
                (true, false) => Color::new(0.2, 0.18, 0.12, 1.0),
                _ => Color::new(0.12, 0.12, 0.14, 1.0),
            };
            draw_rectangle(r.x, r.y, r.w, r.h, bg);
            let fg = if row.enabled { WHITE } else { dim };
            let baseline = r.y + 27.0;
            text(
                &format!("{digit}  {}", row.label),
                r.x + PAD,
                baseline,
                24.0,
                fg,
            );
            let dw = measure_text(&row.detail, None, 24, 1.0).width;
            let price = if row.enabled { GOLD } else { fg };
            text(&row.detail, r.right() - dw - PAD, baseline, 24.0, price);
        }
        text(
            "Number keys or click.  Tab: next page",
            p.x + PAD,
            lay.hint,
            18.0,
            dim,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Something the shop sells that is on no tab can't be bought, and
    /// something on two tabs is clutter.
    #[test]
    fn everything_for_sale_is_on_exactly_one_tab() {
        let game = Game::new(1);
        let listed: Vec<ShopAction> = Tab::ALL
            .iter()
            .flat_map(|&t| rows(&game, t))
            .map(|r| r.action)
            .collect();
        let mut for_sale = vec![
            ShopAction::Sell,
            ShopAction::SellMinerals,
            ShopAction::Refuel,
            ShopAction::Repair,
            ShopAction::BuyBaseKit,
        ];
        for_sale.extend(Upgrade::ALL.map(ShopAction::Buy));
        for_sale.extend(Gun::ALL.map(ShopAction::BuyGun));
        for action in &for_sale {
            let times = listed.iter().filter(|a| *a == action).count();
            assert_eq!(times, 1, "{action:?} is listed {times} times");
        }
        assert_eq!(
            listed.len(),
            for_sale.len(),
            "a row sells something unknown"
        );
    }

    /// The old menu ran out of number keys and left the guns click-only.
    #[test]
    fn every_row_on_every_tab_has_a_number_key() {
        let game = Game::new(1);
        for tab in Tab::ALL {
            let n = rows(&game, tab).len();
            assert!(
                n <= KEYS.len(),
                "{tab:?} has {n} rows for {} keys",
                KEYS.len()
            );
        }
    }

    /// On the smallest window the game supports, 1280 by 720, every tab's
    /// rows sit inside the panel, one under the other, and the panel stays
    /// on screen and out of the left half where the pod is.
    #[test]
    fn every_tab_fits_the_smallest_window() {
        let game = Game::new(1);
        for tab in Tab::ALL {
            let lay = layout(&game, tab, 1280.0);
            let p = lay.panel;
            assert!(
                p.x > 640.0 && p.right() <= 1280.0,
                "{tab:?} panel spans {}..{}",
                p.x,
                p.right()
            );
            assert!(p.bottom() <= 720.0, "{tab:?} panel ends at {}", p.bottom());
            let mut floor = lay.tabs[0].0.bottom();
            for (r, row) in &lay.rows {
                assert!(r.y >= floor, "{} overlaps the row above", row.label);
                assert!(r.x >= p.x && r.right() <= p.right() && r.bottom() <= p.bottom());
                floor = r.bottom();
            }
        }
    }
}
