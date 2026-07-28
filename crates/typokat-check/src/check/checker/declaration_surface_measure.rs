use std::cell::{Cell, RefCell};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct DeclarationSurfaceMeasure {
    pub(super) eligible_interface_property_roots: u64,
    pub(super) eligible_namespace_variable_roots: u64,
    pub(super) eager_materialization_roots: u64,
    pub(super) demand_materialization_roots: u64,
    pub(super) materialization_memo_hits: u64,
    pub(super) materialization_memo_inserts: u64,
}

impl DeclarationSurfaceMeasure {
    pub(super) fn eligible_roots(self) -> u64 {
        self.eligible_interface_property_roots + self.eligible_namespace_variable_roots
    }

    pub(super) fn materialization_roots(self) -> u64 {
        self.eager_materialization_roots + self.demand_materialization_roots
    }
}

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static MEASURE: RefCell<DeclarationSurfaceMeasure> =
        RefCell::new(DeclarationSurfaceMeasure::default());
}

pub(super) struct DeclarationSurfaceMeasureGuard {
    _thread_affine: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl Drop for DeclarationSurfaceMeasureGuard {
    fn drop(&mut self) {
        ACTIVE.with(|active| {
            assert!(
                active.replace(false),
                "declaration-surface measure is active"
            );
        });
        MEASURE.with(|measure| *measure.borrow_mut() = DeclarationSurfaceMeasure::default());
    }
}

pub(super) fn start_declaration_surface_measure() -> DeclarationSurfaceMeasureGuard {
    ACTIVE.with(|active| {
        assert!(
            !active.replace(true),
            "declaration-surface measure cannot nest"
        );
    });
    MEASURE.with(|measure| *measure.borrow_mut() = DeclarationSurfaceMeasure::default());
    DeclarationSurfaceMeasureGuard {
        _thread_affine: std::marker::PhantomData,
    }
}

pub(super) fn declaration_surface_measure() -> Option<DeclarationSurfaceMeasure> {
    ACTIVE
        .with(Cell::get)
        .then(|| MEASURE.with(|measure| *measure.borrow()))
}

fn update(update: impl FnOnce(&mut DeclarationSurfaceMeasure)) {
    if ACTIVE.with(Cell::get) {
        MEASURE.with(|measure| update(&mut measure.borrow_mut()));
    }
}

pub(super) fn record_eager_interface_property_root() {
    update(|measure| {
        measure.eligible_interface_property_roots += 1;
        measure.eager_materialization_roots += 1;
    });
}

pub(super) fn record_eager_namespace_variable_root() {
    update(|measure| {
        measure.eligible_namespace_variable_roots += 1;
        measure.eager_materialization_roots += 1;
    });
}

pub(super) fn record_planned_interface_property_root() {
    update(|measure| measure.eligible_interface_property_roots += 1);
}

pub(super) fn record_planned_namespace_variable_root() {
    update(|measure| measure.eligible_namespace_variable_roots += 1);
}

pub(crate) fn record_demand_materialization_root() {
    update(|measure| measure.demand_materialization_roots += 1);
}

pub(crate) fn record_materialization_memo_hit() {
    update(|measure| measure.materialization_memo_hits += 1);
}

pub(crate) fn record_materialization_memo_insert() {
    update(|measure| measure.materialization_memo_inserts += 1);
}
