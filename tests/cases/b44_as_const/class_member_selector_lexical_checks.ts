declare class SelectorCheckedSource {
  value: { n: number };
}
class SelectorCheckedConsumer {
  source: SelectorCheckedSource = { value: { n: 1 } };
  field = ({ value: this.source.value }) as const;
}
declare const selectorCheckedConsumer: SelectorCheckedConsumer;
selectorCheckedConsumer.field.value.n = 2;
selectorCheckedConsumer.field.value = { n: 2 }; // error[TK2540]
selectorCheckedConsumer.field.value.n = "wrong"; // error[TK2322]

declare class SelectorAccessSource {
  private hidden: number;
  protected inheritedOnly: number;
}
declare const selectorAccessSource: SelectorAccessSource;
class SelectorUnusedInvalidConsumer {
  missing = ({ value: selectorAccessSource.missing }) as const; // error[TK2339]
  hidden = ({ value: selectorAccessSource.hidden }) as const; // error[TK2341]
  protectedValue = ({ value: selectorAccessSource.inheritedOnly }) as const; // error[TK2445]
}
