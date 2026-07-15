// tsc 6.0.3 --strict --noEmit: clean; qualified lookup sees a later ambient reopening.
type ForwardReopeningQualifiedLeaf = Wu3ForwardReopening.Later;

declare namespace Wu3ForwardReopening {
  interface Earlier { earlier: true }
}

declare namespace Wu3ForwardReopening {
  interface Later { later: true }
}
