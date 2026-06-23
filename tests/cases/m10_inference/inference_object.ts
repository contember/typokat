// M10 — inference through a generic object parameter and into a generic return.

interface Box<T> { value: T; }

function unwrap<T>(b: Box<T>): T { return b.value; }

const a: number = unwrap({ value: 1 }); // ok — T inferred number from the argument
const b: string = unwrap({ value: 1 }); // error[TK2322]

function wrap<T>(x: T): Box<T> { return { value: x }; }

const c: Box<number> = wrap(1); // ok — T inferred number, returns Box<number>
const d: Box<string> = wrap(1); // error[TK2322]
