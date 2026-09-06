declare const classPublishedReference: { value: string };

class PublicationRoleBox {
  readonly ordinary = "a";
  readonly exact = "a" as const;
  mutable = "a";
  member = { a: "a" } satisfies { a: "a" | number };
  array = [1] satisfies (string | 1)[];
  tuple = [true] satisfies [number | boolean];
  callback = (() => true) satisfies () => boolean;
  broadCallback = (() => "a") satisfies () => string;
  annotatedCallback = (): boolean => true;
  owned = { nested: { value: "a" } } as const;
  referenced = { nested: classPublishedReference } as const;
}

const publicationRoleBox = new PublicationRoleBox();
let ordinaryClassCopy = publicationRoleBox.ordinary;
ordinaryClassCopy = "b";
let exactClassCopy = publicationRoleBox.exact;
exactClassCopy = "b"; // error[TK2322]
publicationRoleBox.mutable = "b";
let memberClassCopy = publicationRoleBox.member.a;
memberClassCopy = "b"; // error[TK2322]
let arrayClassCopy = publicationRoleBox.array[0];
arrayClassCopy = 2; // error[TK2322]
let tupleClassCopy = publicationRoleBox.tuple[0];
tupleClassCopy = false; // error[TK2322]
let callbackClassCopy = publicationRoleBox.callback();
callbackClassCopy = false; // error[TK2322]
let broadCallbackClassCopy = publicationRoleBox.broadCallback();
broadCallbackClassCopy = "b";
let annotatedCallbackClassCopy = publicationRoleBox.annotatedCallback();
annotatedCallbackClassCopy = false;
publicationRoleBox.owned.nested.value = "b"; // error[TK2540]
publicationRoleBox.referenced.nested.value = "b";
