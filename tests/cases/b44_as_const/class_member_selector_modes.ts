declare const selectorModeSource: SelectorModeSource;
class SelectorModeConsumer {
  field = ({ tag: selectorModeSource.tag, value: selectorModeSource.value }) as const;
}
class SelectorModeSource {
  readonly tag = "a";
  value: { n: number } = { n: 1 };
}
declare const selectorModeConsumer: SelectorModeConsumer;
const selectorModeExact: "a" = selectorModeConsumer.field.tag;
let selectorModeCopy = selectorModeConsumer.field.tag;
selectorModeCopy = "b"; // error[TK2322]
selectorModeConsumer.field.value.n = 2;
selectorModeConsumer.field.value = { n: 2 }; // error[TK2540]
selectorModeConsumer.field.value.n = "wrong"; // error[TK2322]

class SelectorEarlierSource {
  readonly tag = "a";
  value: { n: number } = { n: 1 };
}
declare const selectorEarlierSource: SelectorEarlierSource;
class SelectorLaterConsumer {
  field = ({ tag: selectorEarlierSource.tag, value: selectorEarlierSource.value }) as const;
}
declare const selectorLaterConsumer: SelectorLaterConsumer;
const selectorLaterExact: "a" = selectorLaterConsumer.field.tag;
let selectorLaterCopy = selectorLaterConsumer.field.tag;
selectorLaterCopy = "b"; // error[TK2322]
selectorLaterConsumer.field.value.n = 2;
selectorLaterConsumer.field.value = { n: 2 }; // error[TK2540]
selectorLaterConsumer.field.value.n = "wrong"; // error[TK2322]

declare class SelectorIndexedControl {
  object: { readonly value: SelectorModeSource["value"] };
}
declare const selectorIndexedControl: SelectorIndexedControl;
selectorIndexedControl.object.value.n = 2;
selectorIndexedControl.object.value = { n: 2 }; // error[TK2540]
selectorIndexedControl.object.value.n = "wrong"; // error[TK2322]
