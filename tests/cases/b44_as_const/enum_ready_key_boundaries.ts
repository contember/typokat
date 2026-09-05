// A ready singleton key does not authenticate an aliased or shadowed enum root.

enum ReadyKeyEnum {
  A,
  B,
}

const readyEnumKey = "A";
const readyKeyValue = (ReadyKeyEnum)[(readyEnumKey)] as const;
const exactReadyKeyValue: ReadyKeyEnum.A = readyKeyValue;

declare const publishedEnumKey: "A";
class ReadyKeyField {
  field = ReadyKeyEnum[publishedEnumKey] as const;
}
declare const readyKeyField: ReadyKeyField;
const exactReadyKeyField: ReadyKeyEnum.A = readyKeyField.field;

const readyKeyAlias = ReadyKeyEnum;
const invalidReadyKeyAlias = readyKeyAlias[readyEnumKey] as const; // incomplete[expr-infer/as-const/invalid-operand]

const missingEnumKey = "Missing";
const missingReadyKeyValue = ReadyKeyEnum[missingEnumKey] as const; // incomplete[expr-infer/element-access/implicit-any-index]

function shadowReadyEnum(ReadyKeyEnum: { A: number }) {
  const value = ReadyKeyEnum[readyEnumKey] as const; // incomplete[expr-infer/as-const/invalid-operand]
}

function invalidKeyChild(value: number): "A" {
  return "A";
}
const invalidKeyChildValue = ReadyKeyEnum[invalidKeyChild("wrong")]; // error[TK2345]
