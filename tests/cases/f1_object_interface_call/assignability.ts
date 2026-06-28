// F1 / backlog 05 (WU2) - single call signatures participate in assignment
// compatibility without erasing required named properties.
// Cross-checked against tsc 6.0.3 --strict.

interface InterfaceCallable {
  (input: number): string;
}

type ObjectCallable = {
  (input: number): string;
};

interface OptionalTaggedInterfaceCallable {
  tag?: string;
  (input: number): string;
}

type OptionalTaggedObjectCallable = {
  tag?: string;
  (input: number): string;
};

interface RequiredTaggedInterfaceCallable {
  tag: string;
  (input: number): string;
}

type RequiredTaggedObjectCallable = {
  tag: string;
  (input: number): string;
};

const functionToInterface: InterfaceCallable = (input: number) => "value";                         // ok - no required named properties
const functionToObject: ObjectCallable = (input: number) => "value";                               // ok - no required named properties
const functionToOptionalInterface: OptionalTaggedInterfaceCallable = (input: number) => "value";    // ok - optional property may be absent
const functionToOptionalObject: OptionalTaggedObjectCallable = (input: number) => "value";          // ok - optional property may be absent

declare const interfaceCallable: InterfaceCallable;
declare const objectCallable: ObjectCallable;

const interfaceToFunction: (input: number) => string = interfaceCallable;                           // ok - callable interface is assignable to matching function type
const objectToFunction: (input: number) => string = objectCallable;                                 // ok - callable object type is assignable to matching function type

const missingRequiredInterfaceTag: RequiredTaggedInterfaceCallable = (input: number) => "value";    // error[TK2741]
const missingRequiredObjectTag: RequiredTaggedObjectCallable = (input: number) => "value";          // error[TK2741]
