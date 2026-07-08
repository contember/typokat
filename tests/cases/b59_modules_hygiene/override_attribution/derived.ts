import { Base } from "./base";

export class Derived extends Base {
  m(x: string): string { return x; } // error[TK2416]: Property 'm' in type 'Derived' is not assignable to the same property in base type 'Base'
}
