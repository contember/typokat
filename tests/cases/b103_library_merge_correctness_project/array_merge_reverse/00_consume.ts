const value: number = [1, 2].b103Ordered();
const wrong: string = [1, 2].b103Ordered(); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wrongMap: string[] = [1, 2].map((value) => value + 1); // error[TK2322]
