const reflectedKeys: (string | symbol)[] = Reflect.ownKeys({ value: 1 })
const wrongReflectedKeys: number = Reflect.ownKeys({ value: 1 })
