// Backlog 41 — generic interface/object methods plus generic call and construct
// signatures. Cross-checked with tsc 6.0.3 --strict.

interface Box<T> {
  value: T;
}

interface InterfaceMethods {
  map<U>(value: U): Box<U>;
}

type ObjectMethods = {
  map<U>(value: U): Box<U>;
};

declare const interfaceMethods: InterfaceMethods;
declare const objectMethods: ObjectMethods;

const interfaceMethodResult: Box<number> = interfaceMethods.map(1);
const objectMethodResult: Box<string> = objectMethods.map<string>("value");
const interfaceMethodWrong: Box<string> = interfaceMethods.map(1); // error[TK2322]
objectMethods.map<number>("value"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'

interface Identity {
  <T>(value: T): T;
}

type PairMaker = {
  <T, U = T>(value: T): [T, U];
};

declare const identity: Identity;
declare const pairMaker: PairMaker;

const callInferred: number = identity(1);
const callExplicit: string = identity<string>("value");
const callWrong: string = identity(1); // error[TK2322]
identity<string>(1); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
const callDefault: [number, number] = pairMaker(1);
const callDefaultOverride: [number, string] = pairMaker<number, string>(1);

interface HasId {
  id: number;
}

interface BoundedIdentity {
  <T extends HasId>(value: T): T;
}

declare const boundedIdentity: BoundedIdentity;

const boundedValue: { id: number; label: string } = boundedIdentity({ id: 1, label: "value" });
boundedIdentity<{ label: string }>({ label: "value" }); // error[TK2344]
boundedIdentity("value"); // error[TK2345]

interface BoxConstructor {
  new <T>(value: T): Box<T>;
}

type DefaultBoxConstructor = {
  new <T, U = T>(value: T): Box<U>;
};

declare const GenericBox: BoxConstructor;
declare const DefaultBox: DefaultBoxConstructor;

const constructedInferred: Box<number> = new GenericBox(1);
const constructedExplicit: Box<string> = new GenericBox<string>("value");
const constructedWrong: Box<string> = new GenericBox(1); // error[TK2322]
new GenericBox<string>(1); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
const constructedDefault: Box<number> = new DefaultBox(1);
const constructedDefaultOverride: Box<string> = new DefaultBox<number, string>(1);
