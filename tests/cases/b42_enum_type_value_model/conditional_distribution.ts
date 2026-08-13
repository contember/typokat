declare function nextOpaqueNumber(): number;

enum ConditionalOpen {
  Opaque = nextOpaqueNumber(),
  Fixed = 1,
}

type ExcludeFixed<T> = T extends ConditionalOpen.Fixed ? never : T;
type WithoutFixed = ExcludeFixed<ConditionalOpen>;

const opaqueMember: WithoutFixed = ConditionalOpen.Opaque;
const opaqueRemainder: WithoutFixed = 123;
const excludedMember: WithoutFixed = ConditionalOpen.Fixed; // error[TK2322]

enum ConditionalMemberless {}

type NumberBranch<T> = T extends number ? "number" : "other";
const memberlessStaysAtomic: "number" = null as unknown as NumberBranch<ConditionalMemberless>;
