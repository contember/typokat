// Marker-free demands assert only no extra use-site diagnostic beyond the declaration's backlog-15
// incomplete. Shipped preflight tests pin private routing; backlog 15 retains UMD publication.
const umdValue: number = B14Umd(1);
const umdVersion: string = B14Umd.version;
declare const umdOptions: B14Umd.Options;
const umdEnabled: boolean = umdOptions.enabled;
