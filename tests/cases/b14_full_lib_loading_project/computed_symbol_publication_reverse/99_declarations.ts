// Same declarations as the forward project's first file, after every demand in input order.

interface B14PublishedSymbolItem<T> {
  value: T;
}

interface B14PublishedSymbolOverloaded<T> {
  [Symbol.iterator](kind: "single"): B14PublishedSymbolItem<T>;
}

interface B14PublishedSymbolBase<T> {
  [Symbol.iterator](): B14PublishedSymbolItem<T>;
}

declare class B14PublishedSymbolAugmented<T> {
  value: T;
}
