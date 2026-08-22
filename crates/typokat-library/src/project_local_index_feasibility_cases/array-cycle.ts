declare const concatenable: ConcatArray<number>

const combined: number[] = [1, 2, 3].concat(concatenable)
const sliced: number[] = concatenable.slice()
const wrongCombinedValue: string = combined[0]
