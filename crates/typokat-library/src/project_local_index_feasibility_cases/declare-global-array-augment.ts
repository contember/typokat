export {}

declare global {
	interface Array<T> {
		projectLocalTail(): T
	}
}
