// Backlog 70 — only the trusted prelude declaration gets the intrinsic marker.
type OmitThisParameter<T> = T;
type UserShadowedOmit = OmitThisParameter<
  (this: { tag: "shadow" }, value: number) => void
>;
declare const userShadowedOmit: UserShadowedOmit;
userShadowedOmit(); // error[TK2684] | error[TK2554]
