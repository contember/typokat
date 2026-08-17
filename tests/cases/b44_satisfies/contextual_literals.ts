// tsc 6.0.3 --strict: clean. The target supplies contextual literal shape recursively.

const tuple = [1, 2] satisfies [number, number];
const first: number = tuple[0];

const nested = {
  point: [1, 2],
  child: { name: "ok" },
} satisfies {
  point: readonly [number, number];
  child: { name: string };
};

const mutablePoint: [number, number] = nested.point;
mutablePoint[0] = 3;

const parenthesized = ({ value: 1 }) satisfies { value: number };
const parenthesizedValue: number = parenthesized.value;

const objectUnion = { kind: "a", value: 1 } satisfies
  | { kind: "a"; value: number }
  | { kind: "b"; value: string };
const objectUnionKind: "a" = objectUnion.kind;

const tupleUnion = ["a", 1] satisfies ["a", number] | ["b", string];
const tupleUnionKind: "a" = tupleUnion[0];
