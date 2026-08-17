// Literal-owned members recurse under a const context. The container properties become
// readonly, but a referenced value is inferred normally and is not deep-frozen.

const referencedObject = { mutable: 1 };
const shorthandLiteral: "shorthand" = "shorthand";

const frozenObject = ({
  text: "fixed",
  count: 1,
  flag: true,
  nested: {
    label: "nested",
    list: ["item", { enabled: true }],
  },
  shorthandLiteral,
  referencedObject,
}) as const;

const exactText: "fixed" = frozenObject.text;
const exactCount: 1 = frozenObject.count;
const exactFlag: true = frozenObject.flag;
const exactNestedLabel: "nested" = frozenObject.nested.label;
const exactNestedItem: "item" = frozenObject.nested.list[0];
const exactNestedBoolean: true = frozenObject.nested.list[1].enabled;
const exactShorthand: "shorthand" = frozenObject.shorthandLiteral;

frozenObject.text = "fixed"; // error[TK2540]: Cannot assign to 'text' because it is a read-only property
frozenObject.nested.label = "nested"; // error[TK2540]: Cannot assign to 'label' because it is a read-only property
frozenObject.nested.list[0] = "item"; // error[TK2540]
frozenObject.shorthandLiteral = "shorthand"; // error[TK2540]
frozenObject.referencedObject = referencedObject; // error[TK2540]

// Const propagation stops at the identifier. Its own mutable member remains writable
// and retains ordinary number widening.
frozenObject.referencedObject.mutable = 2;
const referencedLiteralDidNotLeak: 1 = frozenObject.referencedObject.mutable; // error[TK2322]
