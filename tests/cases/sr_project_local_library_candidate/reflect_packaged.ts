const keys: (string | symbol)[] = Reflect.ownKeys({ value: 1 });
const wrongKeys: number = Reflect.ownKeys({ value: 1 }); // error[TK2322]: Type '(string | symbol)[]' is not assignable to type 'number'
