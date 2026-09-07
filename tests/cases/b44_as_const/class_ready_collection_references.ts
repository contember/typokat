declare const collectionObjectSource: { value: { n: number } };
class CollectionObjectControl {
  field = ({ value: collectionObjectSource.value }) as const;
}
declare const collectionObjectControl: CollectionObjectControl;
collectionObjectControl.field.value.n = 2;
collectionObjectControl.field.value = { n: 2 }; // error[TK2540]
collectionObjectControl.field.value.n = "wrong"; // error[TK2322]

class CollectionTupleControl {
  source: [{ n: number }] = [{ n: 1 }];
  field = ({ value: this.source[0] }) as const;
}
declare const collectionTupleControl: CollectionTupleControl;
collectionTupleControl.field.value.n = 2;
collectionTupleControl.field.value = { n: 2 }; // error[TK2540]
collectionTupleControl.field.value.n = "wrong"; // error[TK2322]

class CollectionArrayControl {
  source: { n: number }[] = [{ n: 1 }];
  field = ({ value: this.source[0] }) as const;
}
declare const collectionArrayControl: CollectionArrayControl;
collectionArrayControl.field.value.n = 2;
collectionArrayControl.field.value = { n: 2 }; // error[TK2540]
collectionArrayControl.field.value.n = "wrong"; // error[TK2322]

class CollectionReadonlyTupleControl {
  source: readonly [{ n: number }] = [{ n: 1 }];
  field = ({ value: this.source[0] }) as const;
}
declare const collectionReadonlyTupleControl: CollectionReadonlyTupleControl;
collectionReadonlyTupleControl.field.value.n = 2;
collectionReadonlyTupleControl.field.value = { n: 2 }; // error[TK2540]
collectionReadonlyTupleControl.field.value.n = "wrong"; // error[TK2322]
