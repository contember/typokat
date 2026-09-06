type Alias = { [aliasKey]: unknown; [aliasMethod](): void };
interface Surface { [interfaceKey]: unknown; [interfaceMethod](): void }
const annotated: { [uniqueKey]: string; [methodKey](): void } = source;
typed<{ [nestedKey]: unknown; [nestedMethod](): void }>();
declare function returns(): { [returnKey]: unknown; [returnMethod](): void };
type Mixed<T> = { [valueKey]: T };
type Nested = { [makeKey<KeyType>()]: ValueType };
type Noncomputed = { staticKey: unknown; staticMethod(): void };
