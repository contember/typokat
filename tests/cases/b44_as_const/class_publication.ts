// Unannotated instance fields publish the same complete const-context source type used by
// ordinary expression inference. Static publication remains outside this bounded WU.

class ConstPublished {
  object = ({
    kind: "published",
    nested: { count: 1 },
  }) as const;

  tuple = (["published", { enabled: true }]) as const;
}

declare const constPublished: ConstPublished;

const publishedKind: "published" = constPublished.object.kind;
const publishedCount: 1 = constPublished.object.nested.count;
const publishedTupleKind: "published" = constPublished.tuple[0];
const publishedTupleFlag: true = constPublished.tuple[1].enabled;

constPublished.object.kind = "published"; // error[TK2540]
constPublished.object.nested.count = 1; // error[TK2540]
constPublished.tuple[0] = "published"; // error[TK2540]
constPublished.tuple[1].enabled = true; // error[TK2540]
