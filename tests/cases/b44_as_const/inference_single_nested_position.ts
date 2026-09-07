declare function scalarProject<T>(source: { value: T }): { value: T };
declare function nestedScalarBox<T>(source: T): { value: T };
class WideningScalarHolder { readonly value = "read"; }
const wideningScalarHolder = new WideningScalarHolder();
const projectedScalar = scalarProject(wideningScalarHolder);
const exactProjectedScalar: "read" = projectedScalar.value;
projectedScalar.value = "write"; // error[TK2322]
let projectedScalarCopy = projectedScalar.value;
projectedScalarCopy = "write";
const boxedScalarProperty = nestedScalarBox(wideningScalarHolder.value);
const exactBoxedScalarProperty: "read" = boxedScalarProperty.value; // error[TK2322]
boxedScalarProperty.value = "write";

class RegularScalarHolder { readonly value = "read" as const; }
const regularScalarProjection = scalarProject(new RegularScalarHolder());
const exactRegularProjection: "read" = regularScalarProjection.value;
regularScalarProjection.value = "write"; // error[TK2322]
let regularProjectionCopy = regularScalarProjection.value;
regularProjectionCopy = "write"; // error[TK2322]

enum NestedScalarMode { Read, Write }
const mutableEnumContainer = { value: NestedScalarMode.Read };
const ordinaryEnumProjection = scalarProject(mutableEnumContainer);
const exactOrdinaryEnumProjection: NestedScalarMode.Read = ordinaryEnumProjection.value; // error[TK2322]
ordinaryEnumProjection.value = NestedScalarMode.Write;
const regularEnumContainer = { value: NestedScalarMode.Read as const };
const regularEnumProjection = scalarProject(regularEnumContainer);
const exactRegularEnumProjection: NestedScalarMode.Read = regularEnumProjection.value;
regularEnumProjection.value = NestedScalarMode.Write; // error[TK2322]
let regularEnumProjectionCopy = regularEnumProjection.value;
regularEnumProjectionCopy = NestedScalarMode.Write; // error[TK2322]

declare const NakedScalarConstructor: new <T>(value: T) => T;
declare const NestedScalarConstructor: new <T>(value: T) => { value: T };
declare class ScalarClassBox<T> { constructor(value: T); value: T; }
const constructedScalar = new NakedScalarConstructor("read");
const exactConstructedScalar: "read" = constructedScalar;
const constructedNestedScalar = new NestedScalarConstructor("read");
const exactConstructedNested: "read" = constructedNestedScalar.value; // error[TK2322]
const constructedClassScalar = new ScalarClassBox("read");
const exactConstructedClass: "read" = constructedClassScalar.value; // error[TK2322]
