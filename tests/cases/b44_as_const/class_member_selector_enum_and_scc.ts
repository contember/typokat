enum SelectorFlavor { Plain, Other }
declare const selectorEnumSource: SelectorEnumSource;
class SelectorEnumConsumer {
  field = ({ flavor: selectorEnumSource.flavor }) as const;
}
class SelectorEnumSource {
  readonly flavor = SelectorFlavor.Plain;
}
declare const selectorEnumConsumer: SelectorEnumConsumer;
const selectorEnumExact: SelectorFlavor.Plain = selectorEnumConsumer.field.flavor;
let selectorEnumCopy = selectorEnumConsumer.field.flavor;
selectorEnumCopy = SelectorFlavor.Other; // error[TK2322]
selectorEnumConsumer.field.flavor = SelectorFlavor.Plain; // error[TK2540]

declare const selectorCycleSource: SelectorCycleSource;
class SelectorCycleConsumer {
  field = ({ tag: selectorCycleSource.tag, value: selectorCycleSource.value }) as const;
}
class SelectorCycleSource {
  readonly tag = "cycle";
  value: { n: number } = { n: 1 };
  back: SelectorCycleConsumer | undefined = undefined;
}
declare const selectorCycleConsumer: SelectorCycleConsumer;
const selectorCycleExact: "cycle" = selectorCycleConsumer.field.tag;
let selectorCycleCopy = selectorCycleConsumer.field.tag;
selectorCycleCopy = "other"; // error[TK2322]
selectorCycleConsumer.field.value.n = 2;
selectorCycleConsumer.field.value.n = "wrong"; // error[TK2322]
selectorCycleConsumer.field.value = { n: 2 }; // error[TK2540]
