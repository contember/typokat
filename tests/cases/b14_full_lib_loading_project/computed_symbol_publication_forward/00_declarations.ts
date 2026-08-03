// First half of the same project oracle as computed_symbol_publication_reverse.

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
