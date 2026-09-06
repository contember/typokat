// Shared inference distinguishes top-level returns from nested mutable positions.

enum ReturnPolicyMode { Read = "read", Write = "write" }
declare function directReturnPolicy<T>(value: T): T;
declare function nestedReturnPolicy<T>(value: T): { value: T };

const directEnumPolicy = directReturnPolicy(ReturnPolicyMode.Read);
const directEnumPolicyExact: ReturnPolicyMode.Read = directEnumPolicy;
let mutableEnumPolicy = directReturnPolicy(ReturnPolicyMode.Read);
mutableEnumPolicy = ReturnPolicyMode.Write;
const nestedEnumPolicy = nestedReturnPolicy(ReturnPolicyMode.Read);
const nestedEnumPolicyExact: ReturnPolicyMode.Read = nestedEnumPolicy.value; // error[TK2322]
nestedEnumPolicy.value = ReturnPolicyMode.Write;
const exactNestedEnumPolicy = nestedReturnPolicy(ReturnPolicyMode.Read as const);
const exactNestedEnumPolicyCheck: ReturnPolicyMode.Read = exactNestedEnumPolicy.value;
exactNestedEnumPolicy.value = ReturnPolicyMode.Write; // error[TK2322]

const directStringPolicy = directReturnPolicy("read");
const directStringPolicyExact: "read" = directStringPolicy;
let mutableStringPolicy = directReturnPolicy("read");
mutableStringPolicy = "write";
const nestedStringPolicy = nestedReturnPolicy("read");
const nestedStringPolicyExact: "read" = nestedStringPolicy.value; // error[TK2322]
nestedStringPolicy.value = "write";
const exactNestedStringPolicy = nestedReturnPolicy("read" as const);
const exactNestedStringPolicyCheck: "read" = exactNestedStringPolicy.value;
exactNestedStringPolicy.value = "write"; // error[TK2322]

declare function unionReturnPolicy<T>(value: T): T | undefined;
const unionPolicy = unionReturnPolicy("read");
const exactUnionPolicy: "read" | undefined = unionPolicy;
declare function tupleReturnPolicy<T>(value: T): [T];
const tuplePolicy = tupleReturnPolicy("read");
tuplePolicy[0] = "write";
const exactTuplePolicy: "read" = tuplePolicy[0]; // error[TK2322]
