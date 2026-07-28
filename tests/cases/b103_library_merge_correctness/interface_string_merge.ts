// Backlog 103 correctness: primitive-wrapper augmentation keeps native String members.
interface String {
  b103Upper(): string;
}

const upper: string = "quiet".b103Upper();
const wrongUpper: number = "quiet".b103Upper(); // error[TK2322]: Type 'string' is not assignable to type 'number'
const stringLength: number = "quiet".length;
