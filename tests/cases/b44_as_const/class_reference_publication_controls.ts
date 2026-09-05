// Referenced members stay mutable even when their containing literal-owned field is readonly.

declare const reversedClassSource: ReversedClassSource;
declare const readonlyTupleSource: readonly [{ n: number }];
declare const arrayClassSource: { n: number }[];

class ReversedClassConsumer {
  field = ({ value: reversedClassSource.value }) as const;
  tuple = ({ value: readonlyTupleSource[0] }) as const;
  array = ({ value: arrayClassSource[0] }) as const;
}

declare class ReversedClassSource {
  value: { n: number };
}

declare const reversedClassConsumer: ReversedClassConsumer;
reversedClassConsumer.field.value.n = 2;
reversedClassConsumer.tuple.value.n = 2;
reversedClassConsumer.array.value.n = 2;
reversedClassConsumer.field.value = { n: 2 }; // error[TK2540]
reversedClassConsumer.tuple.value = { n: 2 }; // error[TK2540]
reversedClassConsumer.array.value = { n: 2 }; // error[TK2540]
reversedClassConsumer.field.value.n = "wrong"; // error[TK2322]
reversedClassConsumer.tuple.value.n = "wrong"; // error[TK2322]
reversedClassConsumer.array.value.n = "wrong"; // error[TK2322]
const wrongReferencedValue: string = reversedClassConsumer.field.value.n; // error[TK2322]

declare class RestrictedClassSource {
  private secret: number;
}
declare const restrictedClassSource: RestrictedClassSource;
class InvalidClassReferenceConsumer {
  missing = ({ value: restrictedClassSource.missing }) as const; // error[TK2339]
  private = ({ value: restrictedClassSource.secret }) as const; // error[TK2341]
}
