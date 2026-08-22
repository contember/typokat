interface Array<T> {
	projectLocalOnlyB(): T
}

const projectBValues = [1, 2, 3]
const projectBValue: number = projectBValues.projectLocalOnlyB()
projectBValues.projectLocalOnlyA()
