declare namespace globalThis.Nested { // error[TK2397]: Declaration name conflicts with built-in global identifier 'globalThis'.
  type Missing = DoesNotExist; // error[TK2304]: Cannot find name 'DoesNotExist'
}

declare namespace globalThis { // error[TK2397]: Declaration name conflicts with built-in global identifier 'globalThis'.
  namespace Nested {
    type MissingControl = DoesNotExistControl; // error[TK2304]: Cannot find name 'DoesNotExistControl'
  }
}
