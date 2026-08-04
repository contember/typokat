use typokat_types::types::repr::{DeclaredRecipeId, DeclaredRecipeNode};
use typokat_types::types::{DeclaredMaterializationError, Interner};

#[test]
fn cyclic_declared_recipes_return_typed_errors_without_recursing() {
    let mut self_cycle = Interner::with_intrinsics();
    let self_id = DeclaredRecipeId(
        u32::try_from(self_cycle.store().all_declared_recipes().count())
            .expect("recipe count fits u32"),
    );
    assert_eq!(
        self_cycle.intern_declared_recipe(DeclaredRecipeNode::Array(self_id)),
        self_id
    );
    let self_root = self_cycle.intern_declared(self_id, []);
    assert_eq!(
        self_cycle.materialize_declared(self_root),
        Err(DeclaredMaterializationError::CyclicRecipe(self_id))
    );
    assert_eq!(
        self_cycle.materialize_declared_derived(self_root),
        Err(DeclaredMaterializationError::CyclicRecipe(self_id))
    );

    let mut mutual_cycle = Interner::with_intrinsics();
    let first = DeclaredRecipeId(
        u32::try_from(mutual_cycle.store().all_declared_recipes().count())
            .expect("recipe count fits u32"),
    );
    let second = DeclaredRecipeId(first.0.checked_add(1).expect("second recipe id fits u32"));
    assert_eq!(
        mutual_cycle.intern_declared_recipe(DeclaredRecipeNode::Array(second)),
        first
    );
    assert_eq!(
        mutual_cycle.intern_declared_recipe(DeclaredRecipeNode::Readonly(first)),
        second
    );
    for (recipe, reentry) in [(first, first), (second, second)] {
        let root = mutual_cycle.intern_declared(recipe, []);
        assert_eq!(
            mutual_cycle.materialize_declared(root),
            Err(DeclaredMaterializationError::CyclicRecipe(reentry))
        );
        assert_eq!(
            mutual_cycle.materialize_declared_derived(root),
            Err(DeclaredMaterializationError::CyclicRecipe(reentry))
        );
    }
}
