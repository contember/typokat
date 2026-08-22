export {}

const declaredValues = [1, 2, 3]
const declaredTail: number = declaredValues.projectLocalTail()
const declaredMapped: number[] = declaredValues.map((value) => value + 1)
const declaredArrayConstructor: ArrayConstructor = globalThis.Array
const wrongDeclaredTail: string = declaredValues.projectLocalTail()
