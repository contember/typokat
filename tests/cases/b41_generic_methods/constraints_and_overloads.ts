// Backlog 41 — method-level constraints, defaults, explicit arguments, and
// overload trial isolation. Cross-checked with tsc 6.0.3 --strict.

interface HasId {
  id: number;
}

class Constrained {
  keep<U extends HasId>(value: U): U {
    return value;
  }
}

const constrained = new Constrained();
const constrainedValue: { id: number; label: string } = constrained.keep({ id: 1, label: "value" });
constrained.keep<{ label: string }>({ label: "value" }); // error[TK2344]
constrained.keep("value"); // error[TK2345]

declare class StaticDefaults {
  static pair<T, U = T>(value: T): [T, U];
}

const staticDefault: [number, number] = StaticDefaults.pair(1);
const staticDefaultOverride: [number, string] = StaticDefaults.pair<number, string>(1);

interface GenericOverloads {
  select<T extends number>(value: T): "number";
  select<T extends string>(value: T): "string";
}

declare const genericOverloads: GenericOverloads;

const selectedNumber: "number" = genericOverloads.select(1);
const selectedString: "string" = genericOverloads.select("value");
const selectedWrong: "number" = genericOverloads.select("value"); // error[TK2322]: Type '"string"' is not assignable to type '"number"'
genericOverloads.select(true); // error[TK2769]: No overload matches this call

interface GenericCallOverloads {
  <T extends number>(value: T): { kind: "number"; value: T };
  <T extends string>(value: T): { kind: "string"; value: T };
}

declare const genericCallOverloads: GenericCallOverloads;

const callOverloadNumber: { kind: "number"; value: number } = genericCallOverloads(1);
const callOverloadString: { kind: "string"; value: string } = genericCallOverloads("value");
const callOverloadWrong: { kind: "string"; value: string } = genericCallOverloads(1); // error[TK2322]
genericCallOverloads(true); // error[TK2769]: No overload matches this call

interface GenericConstructOverloads {
  new <T extends number>(value: T): { kind: "number"; value: T };
  new <T extends string>(value: T): { kind: "string"; value: T };
}

declare const GenericOverloadConstructor: GenericConstructOverloads;

const constructOverloadNumber: { kind: "number"; value: number } = new GenericOverloadConstructor(1);
const constructOverloadString: { kind: "string"; value: string } = new GenericOverloadConstructor("value");
const constructOverloadWrong: { kind: "string"; value: string } = new GenericOverloadConstructor(1); // error[TK2322]
new GenericOverloadConstructor(true); // error[TK2769]: No overload matches this call

class GenericClassOverloads {
  select<T extends number>(value: T): "number";
  select<T extends string>(value: T): "string";
  select<T extends number | string>(value: T): "number" | "string" {
    return "number";
  }
}

const genericClassOverloads = new GenericClassOverloads();
const classSelectedNumber: "number" = genericClassOverloads.select(1);
const classSelectedString: "string" = genericClassOverloads.select("value");
const classSelectedWrong: "string" = genericClassOverloads.select(1); // error[TK2322]: Type '"number"' is not assignable to type '"string"'
genericClassOverloads.select(true); // error[TK2769]: No overload matches this call
