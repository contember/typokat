import type { User, Box } from "./model";
import { Token } from "./model";

const badUser: User = { id: "u" }; // error[TK2322]: Type 'string' is not assignable to type 'number'
const badBox: Box<number> = { value: "x" }; // error[TK2322]: Type 'string' is not assignable to type 'number'

class OtherToken {
  private secret: number = 1;
}

const badToken: Token = new OtherToken(); // error[TK2322]
const okToken: Token = new Token();
