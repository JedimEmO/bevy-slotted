//! Menu definitions: slot lists, quick-move routing and synced properties.
//!
//! A [`MenuDef`] is the server-side half of a vanilla container: the ordered
//! list of slots, each pointing at one index of one inventory, plus the rules
//! for shift-click routing and the integer properties the screen displays.
//! It holds no items. The per-open-menu mutable part is [`MenuState`]: the
//! carried stack, the sync counter, an in-progress drag and property values.
//!
//! Slot ids follow the vanilla convention of "order of `addSlot`": container
//! slots first, then player main, then hotbar. [`MenuDef::chest`],
//! [`MenuDef::generic`] and [`MenuDef::player`] reproduce the vanilla layouts.

use serde::{Deserialize, Serialize};

use crate::id::{ItemId, Namespaced};
use crate::inventory::InventoryRef;
use crate::stack::ItemStack;

/// Registry facts the click logic needs but the model does not own.
///
/// The registry crate implements this for a frozen item registry. Tests use a
/// small hand-written table.
pub trait LookupCtx {
    /// Maximum stack size for `id` (64, 16, 1...). Must be at least one.
    fn max_stack(&self, id: ItemId) -> u32;
    /// `true` when `id` carries `tag`.
    fn has_tag(&self, id: ItemId, tag: &Namespaced) -> bool;
}

/// Index of a slot in a [`MenuDef`], the vanilla "slot id".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SlotIx(pub u16);

impl SlotIx {
    /// The area outside every slot; vanilla slot id `-999`. Clicking here
    /// drops the carried stack.
    pub const OUTSIDE: Self = Self(u16::MAX);

    /// `true` for [`OUTSIDE`](Self::OUTSIDE).
    pub const fn is_outside(self) -> bool {
        self.0 == u16::MAX
    }

    /// Position as a `usize` for indexing.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A half-open range of slot ids `start..end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlotRange {
    /// First slot id in the range.
    pub start: u16,
    /// One past the last slot id.
    pub end: u16,
}

impl SlotRange {
    /// The range `start..end`.
    pub const fn new(start: u16, end: u16) -> Self {
        Self { start, end }
    }

    /// `true` when `slot` falls inside.
    pub const fn contains(self, slot: SlotIx) -> bool {
        self.start <= slot.0 && slot.0 < self.end
    }

    /// Iterates the slot ids in order.
    pub fn iter(self) -> impl Iterator<Item = SlotIx> {
        (self.start..self.end).map(SlotIx)
    }
}

/// How a slot reacts to the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SlotBehaviour {
    /// Pick up and place freely, subject to `accepts` and `max_stack`.
    #[default]
    Normal,
    /// A result slot. Items can be taken but never placed. Taking reports
    /// [`Delta::taken_from_output`](crate::click::Delta::taken_from_output)
    /// so the owner can run crafting side effects.
    Output,
    /// A hint slot showing an item that is not really there. Placing sets the
    /// hint without consuming from the carried stack; picking up clears it.
    /// Excluded from item conservation and from every bulk action.
    Ghost,
    /// A player-configured filter. Same interaction as `Ghost`; the two differ
    /// only in how a screen draws them and in what the owner does with them.
    Filter,
    /// Visible but frozen: neither pickup nor placement.
    Locked,
    /// Not part of the menu right now (hidden tab). Invisible to every action.
    Disabled,
}

impl SlotBehaviour {
    /// `true` for `Ghost` and `Filter`, the slots that hold no real items.
    pub const fn is_ghost(self) -> bool {
        matches!(self, Self::Ghost | Self::Filter)
    }
}

/// A serialisable item filter for a slot's `accepts` rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Predicate {
    /// Everything.
    Any,
    /// Exactly this item.
    ItemIs(ItemId),
    /// Any of these items.
    AnyOf(Vec<ItemId>),
    /// Items carrying this tag, resolved through [`LookupCtx::has_tag`].
    Tag(Namespaced),
    /// Negation.
    Not(Box<Predicate>),
}

impl Predicate {
    /// Evaluates the predicate against `id`.
    pub fn matches(&self, id: ItemId, ctx: &dyn LookupCtx) -> bool {
        match self {
            Self::Any => true,
            Self::ItemIs(want) => *want == id,
            Self::AnyOf(ids) => ids.contains(&id),
            Self::Tag(tag) => ctx.has_tag(id, tag),
            Self::Not(inner) => !inner.matches(id, ctx),
        }
    }
}

/// One slot of a menu: where it stores its item and how it behaves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlotDef {
    /// Which inventory backs this slot.
    pub source: InventoryRef,
    /// Which index of that inventory.
    pub index: u16,
    /// Interaction rules.
    #[serde(default)]
    pub behaviour: SlotBehaviour,
    /// A cap below the item's own maximum stack size, if any.
    #[serde(default)]
    pub max_stack: Option<u32>,
    /// What may be placed here. `None` means anything.
    #[serde(default)]
    pub accepts: Option<Predicate>,
}

impl SlotDef {
    /// A `Normal` slot with no cap and no filter.
    pub const fn new(source: InventoryRef, index: u16) -> Self {
        Self {
            source,
            index,
            behaviour: SlotBehaviour::Normal,
            max_stack: None,
            accepts: None,
        }
    }

    /// Builder: set the behaviour.
    #[must_use]
    pub const fn behaviour(mut self, behaviour: SlotBehaviour) -> Self {
        self.behaviour = behaviour;
        self
    }

    /// Builder: cap the stack size.
    #[must_use]
    pub const fn max_stack(mut self, max: u32) -> Self {
        self.max_stack = Some(max);
        self
    }

    /// Builder: restrict what may be placed.
    #[must_use]
    pub fn accepts(mut self, predicate: Predicate) -> Self {
        self.accepts = Some(predicate);
        self
    }

    /// `true` when `stack` may be placed here: a `Normal`, `Ghost` or
    /// `Filter` slot whose `accepts` rule passes.
    pub fn may_place(&self, stack: &ItemStack, ctx: &dyn LookupCtx) -> bool {
        matches!(
            self.behaviour,
            SlotBehaviour::Normal | SlotBehaviour::Ghost | SlotBehaviour::Filter
        ) && self
            .accepts
            .as_ref()
            .is_none_or(|p| p.matches(stack.id, ctx))
    }

    /// `true` when items can be taken out: `Normal` or `Output`.
    pub const fn may_pickup(&self) -> bool {
        matches!(
            self.behaviour,
            SlotBehaviour::Normal | SlotBehaviour::Output
        )
    }

    /// Effective cap for a stack of `id`: the smaller of the slot cap and the
    /// item's own maximum.
    pub fn cap(&self, id: ItemId, ctx: &dyn LookupCtx) -> u32 {
        let item = ctx.max_stack(id).max(1);
        self.max_stack.map_or(item, |m| m.min(item)).max(1)
    }
}

/// Identifier of a synced integer property (furnace progress, anvil cost).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PropertyId(pub u16);

/// A synced integer property and its value when the menu opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyDef {
    /// The property's id. Also its position in [`MenuState::properties`].
    pub id: PropertyId,
    /// Value when the menu opens.
    pub initial: i32,
}

/// One quick-move rule: shift-clicking a slot in `from` tries the menu slots
/// backed by the `to` inventories, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingRule {
    /// Source slots.
    pub from: SlotRange,
    /// Target inventories, tried in order.
    pub to: Vec<InventoryRef>,
    /// Walk the target slots last-to-first, as vanilla does when moving from
    /// a container into the player (the hotbar fills from the right).
    #[serde(default)]
    pub reverse: bool,
}

/// The quick-move (shift-click) routing of a menu. First matching rule wins.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingTable {
    /// Rules in priority order.
    pub rules: Vec<RoutingRule>,
}

impl RoutingTable {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a forward rule.
    #[must_use]
    pub fn route(mut self, from: SlotRange, to: impl Into<Vec<InventoryRef>>) -> Self {
        self.rules.push(RoutingRule {
            from,
            to: to.into(),
            reverse: false,
        });
        self
    }

    /// Adds a rule that fills its targets last-to-first.
    #[must_use]
    pub fn route_reverse(mut self, from: SlotRange, to: impl Into<Vec<InventoryRef>>) -> Self {
        self.rules.push(RoutingRule {
            from,
            to: to.into(),
            reverse: true,
        });
        self
    }

    /// The first rule covering `slot`.
    pub fn resolve_rule(&self, slot: SlotIx) -> Option<&RoutingRule> {
        self.rules.iter().find(|r| r.from.contains(slot))
    }

    /// Target inventories for `slot`, empty when no rule matches.
    pub fn resolve(&self, slot: SlotIx) -> &[InventoryRef] {
        self.resolve_rule(slot).map_or(&[], |r| r.to.as_slice())
    }
}

/// A complete menu definition.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MenuDef {
    /// Slots in id order.
    pub slots: Vec<SlotDef>,
    /// Shift-click routing.
    #[serde(default)]
    pub quick_move: RoutingTable,
    /// Synced integer properties.
    #[serde(default)]
    pub properties: Vec<PropertyDef>,
    /// Fallback for quick-move when no routing rule matches: the inventories
    /// form a ring and a stack moves to the next inventory after its own.
    /// Named after Luanti's `listring[]`.
    #[serde(default)]
    pub listring: Vec<InventoryRef>,
    /// Slots the number keys 1-9 swap with, in key order. Empty when the menu
    /// has no hotbar.
    #[serde(default)]
    pub hotbar: Vec<SlotIx>,
    /// Slot the offhand key swaps with (vanilla `SWAP` button 40).
    #[serde(default)]
    pub offhand: Option<SlotIx>,
}

/// Number of slots in the player's main inventory.
pub const PLAYER_MAIN_SLOTS: u16 = 27;
/// Number of hotbar slots.
pub const PLAYER_HOTBAR_SLOTS: u16 = 9;
/// Number of armor slots.
pub const PLAYER_ARMOR_SLOTS: u16 = 4;

impl MenuDef {
    /// Handle convention of the standard builders: the block's own inventory
    /// (or the crafting result in [`player`](Self::player)).
    pub const CONTAINER: InventoryRef = InventoryRef::new(0);
    /// Handle convention of the standard builders: player main, 27 slots.
    pub const PLAYER_MAIN: InventoryRef = InventoryRef::new(1);
    /// Handle convention of the standard builders: player hotbar, 9 slots.
    pub const PLAYER_HOTBAR: InventoryRef = InventoryRef::new(2);
    /// Handle convention of the standard builders: armor, 4 slots.
    pub const PLAYER_ARMOR: InventoryRef = InventoryRef::new(3);
    /// Handle convention of the standard builders: offhand, 1 slot.
    pub const PLAYER_OFFHAND: InventoryRef = InventoryRef::new(4);
    /// Handle convention of the standard builders: 2x2 crafting grid.
    pub const CRAFT_GRID: InventoryRef = InventoryRef::new(5);

    /// An empty definition.
    pub fn new() -> Self {
        Self::default()
    }

    /// A menu over `container_slots` slots of [`CONTAINER`](Self::CONTAINER)
    /// followed by the player's main inventory and hotbar. Slot ids:
    /// `0..n` container, `n..n+27` main, `n+27..n+36` hotbar.
    ///
    /// Quick-move mirrors vanilla: container slots move into the player
    /// (hotbar first, right to left, then main bottom-up); player slots move
    /// into the container front to back.
    pub fn generic(container_slots: u16) -> Self {
        let mut def = Self::new();
        def.add_slots(Self::CONTAINER, container_slots, SlotBehaviour::Normal);
        let main = def.add_slots(Self::PLAYER_MAIN, PLAYER_MAIN_SLOTS, SlotBehaviour::Normal);
        let hotbar = def.add_slots(
            Self::PLAYER_HOTBAR,
            PLAYER_HOTBAR_SLOTS,
            SlotBehaviour::Normal,
        );
        def.hotbar = hotbar.iter().collect();
        def.quick_move = RoutingTable::new()
            .route_reverse(
                SlotRange::new(0, container_slots),
                [Self::PLAYER_MAIN, Self::PLAYER_HOTBAR],
            )
            .route(SlotRange::new(main.start, hotbar.end), [Self::CONTAINER]);
        def.listring = vec![Self::CONTAINER, Self::PLAYER_MAIN];
        def
    }

    /// A chest of `rows` rows of nine. `chest(3)` is a single chest,
    /// `chest(6)` a double chest.
    pub fn chest(rows: u16) -> Self {
        Self::generic(rows * 9)
    }

    /// The player's own inventory window. Slot ids: `0` crafting result
    /// (`Output`, backed by [`CONTAINER`](Self::CONTAINER)), `1..5` crafting
    /// grid, `5..9` armor (head, chest, legs, feet; capped at one and filtered
    /// by the tags `slotted:armor/head` and so on), `9..36` main, `36..45`
    /// hotbar, `45` offhand.
    ///
    /// Quick-move: main and hotbar swap with each other; everything else goes
    /// to main then hotbar. Vanilla additionally auto-equips armor and shields
    /// on shift-click; that needs equipment data this crate does not have, so
    /// it is left to the owner.
    pub fn player() -> Self {
        let mut def = Self::new();
        let result = def.add_slots(Self::CONTAINER, 1, SlotBehaviour::Output);
        let grid = def.add_slots(Self::CRAFT_GRID, 4, SlotBehaviour::Normal);
        let armor_start = def.slots.len();
        for (i, piece) in ["head", "chest", "legs", "feet"].iter().enumerate() {
            let tag = Namespaced::new("slotted", &format!("armor/{piece}"))
                .expect("static armor tag is valid");
            def.slots.push(
                SlotDef::new(Self::PLAYER_ARMOR, to_u16(i))
                    .max_stack(1)
                    .accepts(Predicate::Tag(tag)),
            );
        }
        let armor = SlotRange::new(to_u16(armor_start), to_u16(def.slots.len()));
        let main = def.add_slots(Self::PLAYER_MAIN, PLAYER_MAIN_SLOTS, SlotBehaviour::Normal);
        let hotbar = def.add_slots(
            Self::PLAYER_HOTBAR,
            PLAYER_HOTBAR_SLOTS,
            SlotBehaviour::Normal,
        );
        let offhand = def.add_slots(Self::PLAYER_OFFHAND, 1, SlotBehaviour::Normal);
        def.hotbar = hotbar.iter().collect();
        def.offhand = Some(SlotIx(offhand.start));
        let player = [Self::PLAYER_MAIN, Self::PLAYER_HOTBAR];
        def.quick_move = RoutingTable::new()
            .route_reverse(result, player)
            .route(SlotRange::new(grid.start, armor.end), player)
            .route(main, [Self::PLAYER_HOTBAR])
            .route(hotbar, [Self::PLAYER_MAIN])
            .route(offhand, player);
        def
    }

    /// Appends `count` slots backed by `source` at indices `0..count` and
    /// returns their slot id range.
    pub fn add_slots(
        &mut self,
        source: InventoryRef,
        count: u16,
        behaviour: SlotBehaviour,
    ) -> SlotRange {
        let start = to_u16(self.slots.len());
        self.slots
            .extend((0..count).map(|index| SlotDef::new(source, index).behaviour(behaviour)));
        SlotRange::new(start, start + count)
    }

    /// The definition of `slot`, if it exists. [`SlotIx::OUTSIDE`] is `None`.
    pub fn slot(&self, slot: SlotIx) -> Option<&SlotDef> {
        self.slots.get(slot.index())
    }

    /// The slot id backed by (`source`, `index`), if any.
    pub fn slot_of(&self, source: InventoryRef, index: u16) -> Option<SlotIx> {
        self.slots
            .iter()
            .position(|s| s.source == source && s.index == index)
            .map(|i| SlotIx(to_u16(i)))
    }

    /// Slot ids backed by `source`, in id order.
    pub fn slots_of(&self, source: InventoryRef) -> impl Iterator<Item = SlotIx> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter(move |(_, s)| s.source == source)
            .map(|(i, _)| SlotIx(to_u16(i)))
    }

    /// The number of slots each inventory needs for this menu: `1 + max
    /// index` per referenced handle, zero for unreferenced handles below the
    /// highest.
    pub fn inventory_sizes(&self) -> Vec<usize> {
        let mut sizes = Vec::new();
        for s in &self.slots {
            let k = s.source.index();
            if sizes.len() <= k {
                sizes.resize(k + 1, 0);
            }
            sizes[k] = sizes[k].max(usize::from(s.index) + 1);
        }
        sizes
    }
}

fn to_u16(n: usize) -> u16 {
    u16::try_from(n).expect("slot count exceeds u16")
}

/// An in-progress drag (vanilla `QUICK_CRAFT`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DragState {
    /// Which distribution the drag performs.
    pub kind: crate::click::DragKind,
    /// Slots painted so far, in paint order, without duplicates.
    pub slots: Vec<SlotIx>,
}

/// The mutable state of one open menu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MenuState {
    /// The stack on the cursor.
    pub carried: Option<ItemStack>,
    /// Incremented by every successful mutating action. The sync layer
    /// compares it to detect prediction mismatches.
    pub state_id: u32,
    /// An in-progress drag, if any.
    pub drag: Option<DragState>,
    /// Current property values, indexed like [`MenuDef::properties`].
    pub properties: Vec<i32>,
    /// What each [`Ghost`](SlotBehaviour::Ghost) and
    /// [`Filter`](SlotBehaviour::Filter) slot displays.
    ///
    /// A hint is a picture of an item, not an item. It lives here rather than
    /// in the backing [`Inventory`](crate::inventory::Inventory) so that
    /// `count_of`, item conservation and inventory sync never see a phantom
    /// stack. Every entry has `count == 1` and every key names a slot of the
    /// menu whose `behaviour.is_ghost()`. Read and write it through
    /// [`hint`](Self::hint) and [`set_hint`](Self::set_hint); the click state
    /// machine keeps it in step, and it travels inside
    /// [`MenuSnapshot`](crate::authority::MenuSnapshot) with the rest of the
    /// state.
    #[serde(default)]
    pub hints: std::collections::BTreeMap<SlotIx, ItemStack>,
}

impl MenuState {
    /// Fresh state for `def`: nothing carried, no hints, properties at their
    /// initial values.
    pub fn new(def: &MenuDef) -> Self {
        Self {
            carried: None,
            state_id: 0,
            drag: None,
            properties: def.properties.iter().map(|p| p.initial).collect(),
            hints: std::collections::BTreeMap::new(),
        }
    }

    /// What the ghost or filter slot `slot` displays, if anything.
    pub fn hint(&self, slot: SlotIx) -> Option<&ItemStack> {
        self.hints.get(&slot)
    }

    /// Sets or clears the hint on `slot`. The stack is stored with `count`
    /// forced to one, because a hint has no quantity.
    pub fn set_hint(&mut self, slot: SlotIx, stack: Option<ItemStack>) {
        match stack {
            Some(stack) => {
                self.hints.insert(slot, stack.with_count(1));
            }
            None => {
                self.hints.remove(&slot);
            }
        }
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::{LookupCtx, MenuDef, Predicate, SlotBehaviour, SlotDef, SlotIx, SlotRange};
    use crate::id::{ItemId, Namespaced};
    use crate::inventory::{Inventories, InventoryRef};
    use crate::stack::ItemStack;

    struct Ctx;

    impl LookupCtx for Ctx {
        fn max_stack(&self, id: ItemId) -> u32 {
            match id.0 {
                1 => 64,
                2 => 16,
                _ => 1,
            }
        }

        fn has_tag(&self, id: ItemId, tag: &Namespaced) -> bool {
            id.0 == 3 && tag.path() == "armor/head"
        }
    }

    #[test]
    fn predicates_evaluate() {
        let helmet = Namespaced::parse("slotted:armor/head").unwrap();
        assert!(Predicate::Any.matches(ItemId(1), &Ctx));
        assert!(Predicate::ItemIs(ItemId(1)).matches(ItemId(1), &Ctx));
        assert!(!Predicate::ItemIs(ItemId(1)).matches(ItemId(2), &Ctx));
        assert!(Predicate::AnyOf(vec![ItemId(1), ItemId(2)]).matches(ItemId(2), &Ctx));
        assert!(Predicate::Tag(helmet.clone()).matches(ItemId(3), &Ctx));
        assert!(!Predicate::Tag(helmet.clone()).matches(ItemId(1), &Ctx));
        assert!(Predicate::Not(Box::new(Predicate::Tag(helmet))).matches(ItemId(1), &Ctx));
    }

    #[test]
    fn slot_cap_is_min_of_slot_and_item() {
        let plain = SlotDef::new(InventoryRef::new(0), 0);
        assert_eq!(plain.cap(ItemId(1), &Ctx), 64);
        assert_eq!(plain.cap(ItemId(2), &Ctx), 16);
        let capped = plain.clone().max_stack(8);
        assert_eq!(capped.cap(ItemId(1), &Ctx), 8);
        assert_eq!(capped.cap(ItemId(9), &Ctx), 1);
    }

    #[test]
    fn behaviours_gate_place_and_pickup() {
        let stone = ItemStack::new(ItemId(1), 1);
        let s = SlotDef::new(InventoryRef::new(0), 0);
        for (b, place, pickup) in [
            (SlotBehaviour::Normal, true, true),
            (SlotBehaviour::Output, false, true),
            (SlotBehaviour::Ghost, true, false),
            (SlotBehaviour::Filter, true, false),
            (SlotBehaviour::Locked, false, false),
            (SlotBehaviour::Disabled, false, false),
        ] {
            let s = s.clone().behaviour(b);
            assert_eq!(s.may_place(&stone, &Ctx), place, "{b:?}");
            assert_eq!(s.may_pickup(), pickup, "{b:?}");
        }
        let filtered = s.accepts(Predicate::ItemIs(ItemId(2)));
        assert!(!filtered.may_place(&stone, &Ctx));
    }

    #[test]
    fn routing_first_match_wins() {
        let def = MenuDef::chest(3);
        assert_eq!(
            def.quick_move.resolve(SlotIx(0)),
            &[MenuDef::PLAYER_MAIN, MenuDef::PLAYER_HOTBAR]
        );
        assert_eq!(def.quick_move.resolve(SlotIx(27)), &[MenuDef::CONTAINER]);
        assert_eq!(def.quick_move.resolve(SlotIx(62)), &[MenuDef::CONTAINER]);
        assert!(def.quick_move.resolve(SlotIx(63)).is_empty());
        assert!(def.quick_move.resolve_rule(SlotIx(0)).unwrap().reverse);
        assert!(!def.quick_move.resolve_rule(SlotIx(30)).unwrap().reverse);
    }

    #[test]
    fn slot_range_contains_half_open() {
        let r = SlotRange::new(2, 5);
        assert!(!r.contains(SlotIx(1)));
        assert!(r.contains(SlotIx(2)));
        assert!(r.contains(SlotIx(4)));
        assert!(!r.contains(SlotIx(5)));
        assert_eq!(r.iter().count(), 3);
    }

    #[test]
    fn inventories_for_menu_fit_every_slot() {
        let def = MenuDef::player();
        assert_eq!(def.inventory_sizes(), vec![1, 27, 9, 4, 1, 4]);
        let invs = Inventories::for_menu(&def);
        assert_eq!(invs.len(), 6);
        assert_eq!(invs[MenuDef::PLAYER_MAIN].len(), 27);
        assert_eq!(Inventories::for_menu(&MenuDef::chest(6)).len(), 3);
    }

    #[test]
    fn slot_lookup_by_source() {
        let def = MenuDef::chest(3);
        assert_eq!(def.slot_of(MenuDef::PLAYER_HOTBAR, 0), Some(SlotIx(54)));
        assert_eq!(def.slot_of(MenuDef::PLAYER_ARMOR, 0), None);
        assert_eq!(def.slots_of(MenuDef::CONTAINER).count(), 27);
        assert!(def.slot(SlotIx::OUTSIDE).is_none());
    }

    #[test]
    fn menu_def_round_trips_through_ron() {
        let def = MenuDef::player();
        let text = ron::to_string(&def).unwrap();
        let back: MenuDef = ron::from_str(&text).unwrap();
        assert_eq!(back, def);
    }
}
