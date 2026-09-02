use crate::schema::{DependencyTarget, LibraryDeclarationProduct};
use std::fmt::Write;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductAuditReceipt {
    pub source_count: usize,
    pub container_count: usize,
    pub declaration_count: usize,
    pub group_count: usize,
    pub region_count: usize,
    pub callable_scope_count: usize,
    pub type_parameter_count: usize,
    pub type_recipe_count: usize,
    pub signature_count: usize,
    pub member_count: usize,
    pub lexical_step_count: usize,
    pub dependency_edge_count: usize,
    pub namespace_attachment_count: usize,
    pub value_attachment_count: usize,
    pub qualified_path_count: usize,
    pub qualified_path_segment_count: usize,
    pub index_signature_parameter_count: usize,
    pub computed_property_name_count: usize,
    pub constructor_count: usize,
    pub private_constructor_count: usize,
    pub abstract_class_count: usize,
    pub abstract_member_count: usize,
    pub resolved_group_reference_count: usize,
    pub type_parameter_reference_count: usize,
    pub unresolved_reference_count: usize,
    pub resolved_group_dependency_count: usize,
    pub type_parameter_dependency_count: usize,
    pub unresolved_dependency_count: usize,
    pub canonical_bytes: usize,
    pub canonical_digest: String,
    pub dependency_edge_digest: String,
    pub dependency_scc_digest: String,
    pub lexical_order_digest: String,
    pub callable_scope_digest: String,
    pub namespace_attachment_digest: String,
    pub value_attachment_digest: String,
    pub qualified_path_digest: String,
}

impl LibraryDeclarationProduct {
    pub fn canonical_audit_render(&self) -> String {
        let mut output = String::new();
        let _ = writeln!(output, "profile={}", self.profile_identity());
        for (row, value) in self.strings.iter().enumerate() {
            let _ = writeln!(output, "string|{row}|{value:?}");
        }
        render_rows(&mut output, "source", &self.sources);
        render_rows(&mut output, "container", &self.containers);
        render_rows(&mut output, "region", &self.regions);
        render_rows(&mut output, "callable_scope", &self.callable_scopes);
        render_rows(&mut output, "declaration", &self.declarations);
        render_rows(&mut output, "group", &self.groups);
        render_rows(&mut output, "namespace", &self.namespaces);
        render_rows(&mut output, "value", &self.values);
        render_rows(&mut output, "intrinsic_marker", &self.intrinsic_markers);
        render_rows(&mut output, "qualified_path", &self.qualified_paths);
        render_rows(
            &mut output,
            "qualified_path_segment",
            &self.qualified_path_segments,
        );
        render_rows(&mut output, "type_parameter", &self.type_parameters);
        render_rows(&mut output, "type", &self.types);
        render_rows(&mut output, "signature", &self.signatures);
        render_rows(&mut output, "member", &self.members);
        render_rows(&mut output, "dependency", &self.dependencies);
        render_rows(&mut output, "lexical", &self.lexical_steps);
        output
    }

    pub fn audit_receipt(&self) -> ProductAuditReceipt {
        let canonical = self.canonical_audit_render();
        ProductAuditReceipt {
            source_count: self.sources.len(),
            container_count: self.containers.len(),
            declaration_count: self.declarations.len(),
            group_count: self.groups.len(),
            region_count: self.regions.len(),
            callable_scope_count: self.callable_scopes.len(),
            type_parameter_count: self.type_parameters.len(),
            type_recipe_count: self.types.len(),
            signature_count: self.signatures.len(),
            member_count: self.members.len(),
            lexical_step_count: self.lexical_steps.len(),
            dependency_edge_count: self.dependencies.len(),
            namespace_attachment_count: self.namespaces.len(),
            value_attachment_count: self.values.len(),
            qualified_path_count: self.qualified_paths.len(),
            qualified_path_segment_count: self.qualified_path_segments.len(),
            index_signature_parameter_count: self
                .members
                .iter()
                .filter(|member| member.kind == crate::schema::MemberKind::Index)
                .filter_map(|member| member.signature)
                .map(|signature| self.signatures[signature.index()].parameters.len())
                .sum(),
            computed_property_name_count: self
                .members
                .iter()
                .filter(|member| {
                    matches!(
                        &member.name,
                        Some(crate::schema::PropertyNameRecipe::Computed { site: _, value: _ })
                    )
                })
                .count(),
            constructor_count: self
                .members
                .iter()
                .filter(|member| member.kind == crate::schema::MemberKind::Constructor)
                .count(),
            private_constructor_count: self
                .members
                .iter()
                .filter(|member| {
                    member.kind == crate::schema::MemberKind::Constructor
                        && member.accessibility == Some(crate::schema::Accessibility::Private)
                })
                .count(),
            abstract_class_count: self
                .declarations
                .iter()
                .filter(|declaration| declaration.flags.r#abstract)
                .count(),
            abstract_member_count: self
                .members
                .iter()
                .filter(|member| member.r#abstract)
                .count(),
            resolved_group_reference_count: self
                .types
                .iter()
                .filter(|recipe| {
                    matches!(
                        &recipe.node,
                        crate::schema::TypeRecipeNode::Reference {
                            resolution: crate::schema::TypeReferenceResolution::Group(_),
                            ..
                        }
                    )
                })
                .count(),
            type_parameter_reference_count: self
                .types
                .iter()
                .filter(|recipe| {
                    matches!(
                        &recipe.node,
                        crate::schema::TypeRecipeNode::Reference {
                            resolution: crate::schema::TypeReferenceResolution::TypeParameter(_),
                            ..
                        }
                    )
                })
                .count(),
            unresolved_reference_count: self
                .types
                .iter()
                .filter(|recipe| {
                    matches!(
                        &recipe.node,
                        crate::schema::TypeRecipeNode::Reference {
                            resolution: crate::schema::TypeReferenceResolution::Unresolved { .. },
                            ..
                        }
                    )
                })
                .count(),
            resolved_group_dependency_count: self
                .dependencies
                .iter()
                .filter(|dependency| matches!(&dependency.target, DependencyTarget::Group(_)))
                .count(),
            type_parameter_dependency_count: self
                .dependencies
                .iter()
                .filter(|dependency| {
                    matches!(&dependency.target, DependencyTarget::TypeParameter(_))
                })
                .count(),
            unresolved_dependency_count: self
                .dependencies
                .iter()
                .filter(|dependency| {
                    matches!(&dependency.target, DependencyTarget::Unresolved { .. })
                })
                .count(),
            canonical_bytes: canonical.len(),
            canonical_digest: sha256_hex(canonical.as_bytes()),
            dependency_edge_digest: digest_rows(&self.dependencies),
            dependency_scc_digest: sha256_hex(dependency_scc_rows(self).as_bytes()),
            lexical_order_digest: digest_rows(&self.lexical_steps),
            callable_scope_digest: digest_rows(&self.callable_scopes),
            namespace_attachment_digest: digest_rows(&self.namespaces),
            value_attachment_digest: digest_rows(&self.values),
            qualified_path_digest: qualified_path_digest(self),
        }
    }

    /// Canonical dependency SCCs over immutable product group keys.
    pub fn dependency_sccs(&self) -> Vec<Vec<crate::GroupKey>> {
        dependency_scc_indices(self)
            .into_iter()
            .map(|component| {
                component
                    .into_iter()
                    .map(|index| crate::GroupKey::new(index).expect("validated group index"))
                    .collect()
            })
            .collect()
    }
}

fn qualified_path_digest(product: &LibraryDeclarationProduct) -> String {
    let mut text = String::new();
    for (row, value) in product.qualified_paths.iter().enumerate() {
        let _ = writeln!(text, "path|{row}|{value:?}");
    }
    for (row, value) in product.qualified_path_segments.iter().enumerate() {
        let _ = writeln!(text, "segment|{row}|{value:?}");
    }
    sha256_hex(text.as_bytes())
}

fn render_rows<T: std::fmt::Debug>(output: &mut String, table: &str, rows: &[T]) {
    for (row, value) in rows.iter().enumerate() {
        let _ = writeln!(output, "{table}|{row}|{value:?}");
    }
}

fn digest_rows<T: std::fmt::Debug>(rows: &[T]) -> String {
    let mut text = String::new();
    for (row, value) in rows.iter().enumerate() {
        let _ = writeln!(text, "{row}|{value:?}");
    }
    sha256_hex(text.as_bytes())
}

fn dependency_scc_rows(product: &LibraryDeclarationProduct) -> String {
    let components = dependency_scc_indices(product);
    let mut output = String::new();
    for component in components {
        let _ = writeln!(output, "{component:?}");
    }
    output
}

#[cfg(test)]
std::thread_local! {
    static DEPENDENCY_SCC_COMPUTATION_COUNT: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn dependency_scc_computation_count() -> usize {
    DEPENDENCY_SCC_COMPUTATION_COUNT.get()
}

fn dependency_scc_indices(product: &LibraryDeclarationProduct) -> Vec<Vec<usize>> {
    #[cfg(test)]
    DEPENDENCY_SCC_COMPUTATION_COUNT
        .set(dependency_scc_computation_count().checked_add(1).expect("SCC computation count"));

    let mut edges = vec![Vec::new(); product.groups.len()];
    for dependency in &product.dependencies {
        if let DependencyTarget::Group(target) = dependency.target {
            edges[dependency.owner.index()].push(target.index());
        }
    }
    for targets in &mut edges {
        targets.sort_unstable();
        targets.dedup();
    }
    let mut reverse = vec![Vec::new(); edges.len()];
    for (source, targets) in edges.iter().enumerate() {
        for target in targets {
            reverse[*target].push(source);
        }
    }
    let mut visited = vec![false; edges.len()];
    let mut order = Vec::with_capacity(edges.len());
    for node in 0..edges.len() {
        finish_order(node, &edges, &mut visited, &mut order);
    }
    visited.fill(false);
    let mut components = Vec::new();
    for node in order.into_iter().rev() {
        if visited[node] {
            continue;
        }
        let mut component = Vec::new();
        collect_component(node, &reverse, &mut visited, &mut component);
        component.sort_unstable();
        components.push(component);
    }
    components.sort();
    components
}

fn finish_order(node: usize, edges: &[Vec<usize>], visited: &mut [bool], order: &mut Vec<usize>) {
    if visited[node] {
        return;
    }
    visited[node] = true;
    for target in &edges[node] {
        finish_order(*target, edges, visited, order);
    }
    order.push(node);
}

fn collect_component(
    node: usize,
    edges: &[Vec<usize>],
    visited: &mut [bool],
    component: &mut Vec<usize>,
) {
    if visited[node] {
        return;
    }
    visited[node] = true;
    component.push(node);
    for target in &edges[node] {
        collect_component(*target, edges, visited, component);
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut state = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = u64::try_from(bytes.len())
        .unwrap_or(u64::MAX)
        .wrapping_mul(8);
    let mut padded = bytes.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in padded.chunks_exact(64) {
        compress(&mut state, chunk);
    }
    let mut output = String::with_capacity(64);
    for word in state {
        let _ = write!(output, "{word:08x}");
    }
    output
}

fn compress(state: &mut [u32; 8], chunk: &[u8]) {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut words = [0u32; 64];
    for (index, word) in words[..16].iter_mut().enumerate() {
        let start = index * 4;
        *word = u32::from_be_bytes([
            chunk[start],
            chunk[start + 1],
            chunk[start + 2],
            chunk[start + 3],
        ]);
    }
    for index in 16..64 {
        let s0 = words[index - 15].rotate_right(7)
            ^ words[index - 15].rotate_right(18)
            ^ (words[index - 15] >> 3);
        let s1 = words[index - 2].rotate_right(17)
            ^ words[index - 2].rotate_right(19)
            ^ (words[index - 2] >> 10);
        words[index] = words[index - 16]
            .wrapping_add(s0)
            .wrapping_add(words[index - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for index in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choice = (e & f) ^ ((!e) & g);
        let first = h
            .wrapping_add(s1)
            .wrapping_add(choice)
            .wrapping_add(K[index])
            .wrapping_add(words[index]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let second = s0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(first);
        d = c;
        c = b;
        b = a;
        a = first.wrapping_add(second);
    }
    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

#[cfg(test)]
mod tests {
    use super::dependency_scc_computation_count;
    use crate::{
        ContainerKind, ContainerRecipe, DeclarationGroupRecipe, DeclarationSlots, GroupKey,
        ProductAssembly, RegionKind, RegionRecipe, SourceClassification, SourceRecipe, SourceSite,
    };

    struct DependencySccComputationScope {
        initial_count: usize,
    }

    impl DependencySccComputationScope {
        fn start() -> Self {
            Self {
                initial_count: dependency_scc_computation_count(),
            }
        }

        fn observed(&self) -> usize {
            dependency_scc_computation_count()
                .checked_sub(self.initial_count)
                .expect("SCC computation counter must not move backwards")
        }
    }

    #[test]
    fn immutable_product_reuses_dependency_sccs_after_construction() {
        let mut assembly = ProductAssembly::new();
        let source = assembly.next_source_key().expect("source key");
        let root_region = assembly.next_region_key().expect("root region key");
        let container = assembly
            .add_container(ContainerRecipe {
                parent: None,
                kind: ContainerKind::CompilationGlobal,
                name: None,
                anchor: None,
            })
            .expect("container");
        let source_name = assembly.intern("scc.d.ts").expect("source name");
        let source_digest = assembly.intern("00").expect("source digest");
        assembly
            .add_source(SourceRecipe {
                ordinal: 0,
                name: source_name,
                sha256: source_digest,
                byte_len: 1,
                classification: SourceClassification::Script,
                root_region,
            })
            .expect("source");
        assembly
            .add_region(RegionRecipe {
                parent: None,
                container,
                kind: RegionKind::Source,
                site: SourceSite {
                    source,
                    start: 0,
                    end: 1,
                },
            })
            .expect("root region");
        let groups = ["First", "Second", "Third"].map(|name| {
            let name = assembly.intern(name).expect("group name");
            assembly
                .add_group(DeclarationGroupRecipe {
                    container,
                    name,
                    slots: DeclarationSlots::default(),
                    fragments: Box::new([]),
                })
                .expect("group")
        });
        let product = assembly.finish("scc-cache-test").expect("valid product");
        let computation_scope = DependencySccComputationScope::start();

        let first = product.dependency_sccs();
        let second = product.dependency_sccs();
        let first: &[Vec<GroupKey>] = first.as_ref();
        let second: &[Vec<GroupKey>] = second.as_ref();
        let expected = [
            vec![groups[0]],
            vec![groups[1]],
            vec![groups[2]],
        ];

        assert_eq!(first, expected.as_slice());
        assert_eq!(second, expected.as_slice());
        assert_eq!(first, second);
        assert_eq!(
            computation_scope.observed(),
            0,
            "immutable product dependency SCCs must be computed during ProductAssembly::finish \
             and reused by checker sessions"
        );
    }
}
