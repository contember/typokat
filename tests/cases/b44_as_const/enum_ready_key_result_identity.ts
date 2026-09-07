enum ResultKeyEnum { A, B }
const resultKeyName = "A";
const resultKeyValue = (ResultKeyEnum)[(resultKeyName)] as const;
const resultKeyExact: ResultKeyEnum.A = resultKeyValue;
const resultKeyWrong: ResultKeyEnum.B = resultKeyValue; // error[TK2322]

declare const resultPublishedKey: "A";
class ResultKeyConsumer {
  field = ResultKeyEnum[resultPublishedKey] as const;
}
declare const resultKeyConsumer: ResultKeyConsumer;
const resultKeyFieldExact: ResultKeyEnum.A = resultKeyConsumer.field;
const resultKeyFieldWrong: ResultKeyEnum.B = resultKeyConsumer.field; // error[TK2322]
