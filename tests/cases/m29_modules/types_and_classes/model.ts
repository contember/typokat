export interface User {
  id: number;
  name?: string;
}

export type Box<T> = { value: T };

export class Token {
  private secret: number = 1;
}
