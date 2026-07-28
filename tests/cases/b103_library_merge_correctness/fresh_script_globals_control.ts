// Backlog 103 control: unrelated script globals still publish through the ordinary delta.
interface B103FreshShape {
  label: string;
}

declare var b103FreshValue: B103FreshShape;
declare function b103FreshCall(input: string): number;

const label: string = b103FreshValue.label;
const wrongLabel: number = b103FreshValue.label; // error[TK2322]: Type 'string' is not assignable to type 'number'
const called: number = b103FreshCall("x");
