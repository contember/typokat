enum Repeated {
  First = 1,
}
enum Repeated {
  Second = 2,
}

const repeatedWhole: Repeated = Repeated.Second;
const repeatedMember: Repeated.Second = Repeated.Second;

enum EnumThenNamespace {
  Member = "member",
}
namespace EnumThenNamespace {
  export const helper = 1;
  export interface Payload {
    value: EnumThenNamespace.Member;
  }
}

const enumThenMember: EnumThenNamespace = EnumThenNamespace.Member;
const enumThenHelper: number = EnumThenNamespace.helper;
const enumThenPayload: EnumThenNamespace.Payload = { value: EnumThenNamespace.Member };

namespace NamespaceThenEnum {
  export const helper = "helper";
  export interface Payload {
    value: NamespaceThenEnum.Member;
  }
}
enum NamespaceThenEnum {
  Member = 1,
}

const namespaceThenMember: NamespaceThenEnum = NamespaceThenEnum.Member;
const namespaceThenHelper: string = NamespaceThenEnum.helper;
const namespaceThenPayload: NamespaceThenEnum.Payload = { value: NamespaceThenEnum.Member };

enum MissingLaterInitializer {
  First,
}
enum MissingLaterInitializer {
  Second, // error[TK2432]
}

enum DuplicateMember {
  Same = 1, // error[TK2300]
}
enum DuplicateMember {
  Same = 2, // error[TK2300]
}
