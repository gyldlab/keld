"""Definition-only identity vectors. Not product code or native qualification."""
from __future__ import annotations
import copy
import hashlib
import json
import re
import struct
from pathlib import Path
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

ROOT = Path(__file__).resolve().parent
DOMAIN1 = b"keld.expected-app-identity/v1\0"
DOMAIN2 = b"keld.expected-app-identity/v2\0"
PROVIDER = "microsoft-artifact-signing-public-trust"
CAT_DOMAIN = b"KELD-WINDOWS-INITIAL-CATALOG-v2\0"
TX_DOMAIN = b"KELD-WINDOWS-IDENTITY-TRANSITION-v1\0"
H = lambda text: hashlib.sha256(text.encode()).hexdigest()
APP = re.compile(r"[a-z0-9](?:[a-z0-9-]*[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)*\Z")
TESTS = []

def require(ok, reason="invalid"):
    if not ok:
        raise ValueError(reason)

def oid(value):
    require(isinstance(value, str) and value.isascii() and 3 <= len(value) <= 255, "oid bytes")
    parts = value.split(".")
    require(2 <= len(parts) <= 64, "oid arcs")
    require(all(re.fullmatch(r"0|[1-9][0-9]*", p) for p in parts), "oid spelling")
    nums = [int(p) for p in parts]
    require(all(n <= 0xffffffff for n in nums), "oid overflow")
    require(nums[0] <= 2 and (nums[0] == 2 or nums[1] <= 39), "oid roots")

def text_field(value, maximum):
    require(isinstance(value, str), "text type")
    raw = value.encode("utf-8")
    require(1 <= len(raw) <= maximum, "text bound")
    require(not any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in value), "control")
    return raw

def lp8(value, maximum):
    raw = text_field(value, maximum)
    return bytes([len(raw)]) + raw

def encode_v1(f):
    return DOMAIN1 + lp8(f["appId"], 255) + lp8(f["channel"], 16) + lp8(f["target"], 64) + bytes.fromhex(f["updatePublicKeyEd25519"])

def encode_v2(f):
    o = f["profileOid"].encode("ascii")
    oid(f["profileOid"])
    return (DOMAIN2 + lp8(f["appId"], 255) + lp8(f["channel"], 16) + lp8(f["target"], 64)
            + bytes.fromhex(f["updatePublicKeyEd25519"]) + b"\x01" + struct.pack(">H", len(o)) + o
            + b"".join(bytes.fromhex(f[n]) for n in ["issuerPolicySha256", "repositoryId", "repositoryPolicySha256", "discoveryPolicySha256"])
            + bytes([1, 1, 1, 2]))

def decode_payload(raw, version):
    domain = DOMAIN1 if version == 1 else DOMAIN2
    require(raw.startswith(domain), "domain")
    cursor = len(domain)
    offsets = {}
    def take(n):
        nonlocal cursor
        require(cursor + n <= len(raw), "truncated")
        result = raw[cursor:cursor+n]
        cursor += n
        return result
    fields = {}
    for name, cap in [("appId", 255), ("channel", 16), ("target", 64)]:
        offsets[name] = cursor
        length = take(1)[0]
        fields[name] = take(length).decode("utf-8", "strict")
        text_field(fields[name], cap)
    fields["updatePublicKeyEd25519"] = take(32).hex()
    if version == 2:
        offsets["provider"] = cursor
        require(take(1) == b"\x01", "provider")
        offsets["profileOid"] = cursor
        length = int.from_bytes(take(2), "big")
        require(3 <= length <= 255, "oid bound")
        fields["profileOid"] = take(length).decode("ascii", "strict")
        oid(fields["profileOid"])
        for n in ["issuerPolicySha256", "repositoryId", "repositoryPolicySha256", "discoveryPolicySha256"]:
            fields[n] = take(32).hex()
        for n, value in [("owner", 1), ("installMode", 1), ("principalModel", 1), ("rootPolicy", 2)]:
            offsets[n] = cursor
            require(take(1) == bytes([value]), n)
    require(cursor == len(raw), "trailing bytes")
    return fields, offsets

def primitive(kind):
    def validate(v):
        if kind == "h32" or kind == "key32":
            require(isinstance(v, str) and re.fullmatch("[0-9a-f]{64}", v), kind)
        elif kind == "app":
            require(isinstance(v, str) and 1 <= len(v) <= 255 and APP.fullmatch(v), kind)
        elif kind == "channel":
            require(v in ["stable", "beta", "canary"], kind)
        elif kind == "target":
            require(v == "windows-x64", kind)
        elif kind == "n53":
            require(type(v) is int and 1 <= v <= 9007199254740991, kind)
        elif kind == "version":
            require(isinstance(v, str) and 1 <= len(v) <= 255 and re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?", v), kind)
            if "-" in v.split("+")[0]:
                pre = v.split("+")[0].split("-",1)[1]
                require(all(not x.isdigit() or x == "0" or not x.startswith("0") for x in pre.split(".")), kind)
        elif kind == "oid":
            oid(v)
        elif kind == "url":
            require(isinstance(v, str) and 1 <= len(v) <= 2048 and re.fullmatch(r"https://[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?/(?:[A-Za-z0-9._~-]+/)*",v), kind)
            require(all(p not in [".",".."] for p in v.split("/")[3:]), kind)
        elif kind == "text":
            text_field(v, 32767)
        else:
            require(v == kind and (not isinstance(kind,int) or type(v) is int), "enum " + str(kind))
    return validate

def obj(fields):
    def validate(v):
        require(type(v) is dict and list(v) == [k for k,_ in fields], "closed fields/order")
        for k, rule in fields:
            (primitive(rule) if isinstance(rule,(str,int)) else rule)(v[k])
    return validate

def arr(rule, lo, hi, unique=False, sorted_values=False):
    def validate(v):
        require(type(v) is list and lo <= len(v) <= hi, "array bound")
        for x in v:
            (primitive(rule) if isinstance(rule,str) else rule)(x)
        if unique:
            require(len({json.dumps(x,separators=(",",":")) for x in v}) == len(v), "duplicate array entry")
        if sorted_values:
            require(v == sorted(v), "array sort")
    return validate

PUB = obj([("schema","keld.publisher-policy/v2"),("provider",PROVIDER),("profileOid","oid"),("issuerPolicySha256","h32")])
ISSUER = obj([("schema","keld.public-trust-issuer-policy/v1"),("chainPolicy","windows-generic-verify-v2"),("rootCertificateSha256",arr("h32",1,8,True,True)),("leafIssuerSpkiSha256",arr("h32",1,8,True,True))])
KEY = obj([("keyId","h32"),("publicKeyEd25519","key32")])
REPO = obj([("schema","keld.repository-policy/v1"),("repositoryId","h32"),("appId","app"),("metadataBaseUrl","url"),("targetsBaseUrl","url"),("bootstrapRootSha256","h32"),("bootstrapRootVersion","n53"),("metadataProfile",1),("target","target"),("catalogueKeys",arr(KEY,1,4,True)),("publisherPolicySha256","h32"),("discoveryPolicySha256","h32"),("recoveryPolicySha256","h32")])
def discovery_entry(v):
    require(type(v) is dict and v.get("identityScheme") in ["spki-v1","public-trust-profile-v2"], "discovery scheme")
    scheme = v["identityScheme"]
    root = "keld-localappdata-app-v1" if scheme == "spki-v1" else "keld-localappdata-app-v2"
    require(v.get("ownerProducer") in ["keld-provenance-v2","keld-provenance-v3"], "owner producer")
    obj([("identityScheme",scheme),("publisherScope","h32"),("appId","app"),("channel","channel"),("target","target"),("rootPolicy",root),("ownerProducer",v["ownerProducer"])])(v)
DISCOVERY = obj([("schema","keld.windows.discovery-policy/v1"),("repositoryId","h32"),("appId","app"),("entries",arr(discovery_entry,1,32,True))])
HOST = obj([("sha256","h32"),("bytes","n53")])
PACKAGE = obj([(n,"version" if n=="version" else "n53" if n.endswith("Bytes") else "h32") for n in ["version","manifestSha256","manifestBytes","manifestSignatureSha256","manifestSignatureBytes","transportBlake3","transportBytes","contentBlake3","contentBytes","profileDigest"]])
INTENT = obj([("owner","direct"),("mode","per-user-direct"),("principalModel","strict-distinct-os-principals"),("rootPolicy","keld-localappdata-app-v2")])
CAT = obj([("schema","keld.windows.initial-catalogue/v2"),("repositoryId","h32"),("repositoryPolicySha256","h32"),("catalogueKeyId","h32"),("appId","app"),("channel","channel"),("target","target"),("publisherPolicy",PUB),("discoveryPolicySha256","h32"),("host",HOST),("package",PACKAGE),("updatePublicKeyEd25519","key32"),("installIntent",INTENT)])
def identity_binding(v):
    require(type(v) is dict and v.get("scheme") in ["spki-v1","public-trust-profile-v2"], "identity scheme")
    fields = [("scheme",v["scheme"]),("publisherScope","h32")]
    if v["scheme"] == "public-trust-profile-v2": fields += [("publisherPolicySha256","h32")]
    obj(fields)(v)
ARTIFACT_FIELDS=[("version","version"),("contentBlake3","h32"),("hostSha256","h32"),("helperSha256","h32"),("manifestSha256","h32"),("updatePublicKeyEd25519","key32"),("hostSpkiSha256","h32"),("identity",identity_binding)]
ARTIFACT = obj(ARTIFACT_FIELDS)
DESTINATION = obj([("catalogueSha256","h32")]+ARTIFACT_FIELDS)
TX = obj([("schema","keld.windows.identity-transition/v1"),("repositoryId","h32"),("appId","app"),("channel","channel"),("target","target"),("source",ARTIFACT),("destination",DESTINATION),("rollback",ARTIFACT),("browserStore",obj([("mode","retain-source"),("profileIdentity","h32")])),("installMode","per-user-direct"),("principalModel","strict-distinct-os-principals")])
WIRE_ARTIFACT=obj([("app_id","app"),("channel","channel"),("target","target"),("version","version"),("content_blake3","h32")])
WIRE_IDENTITY=obj([("install_mode","per-user-direct"),("app_id","app"),("channel","channel"),("target","target"),("install_root","text"),("update_root","text"),("signing_key_id","h32"),("baseline",WIRE_ARTIFACT),("profile_digest","h32"),("principal_model","strict-distinct-os-principals")])
def browser_binding(v):
    require(type(v) is dict and v.get("kind") in ["fresh-v2","retained-predecessor"], "browser kind")
    fields=[("kind",v["kind"]),("profile_identity","h32")]
    if v["kind"]=="retained-predecessor": fields += [("owner_record_sha256","h32"),("predecessor",discovery_entry)]
    obj(fields)(v)
SIGNER=obj([("sha256","h32"),("spki_sha256","h32")])
RECOVERY_BINDING=obj([("artifact",ARTIFACT),("prior_provenance_sha256","h32"),("authorization",obj([("kind",lambda v:require(v in ["planned-transition","manual-recovery"],"authorization kind")),("sha256","h32")])),("profile_identity","h32")])
PROVENANCE=obj([("schema","keld.install-provenance/v3"),("owner","direct"),("protection_profile","windows-per-user-role-rx-v1"),("identity",WIRE_IDENTITY),("publisher",obj([("scheme","public-trust-profile-v2"),("scope","h32"),("policy",PUB)])),("volume_guid","text"),("repository_id","h32"),("repository_policy_sha256","h32"),("discovery_policy_sha256","h32"),("browser_binding",browser_binding),("baseline_signers",obj([("host",SIGNER),("helper",SIGNER)])),("recovery_bindings",arr(RECOVERY_BINDING,0,1))])
RULES={"issuer":ISSUER,"publisher":PUB,"repository":REPO,"discovery":DISCOVERY,"catalogue":CAT,"transition":TX,"provenance":PROVENANCE}

def canonical(v):
    return json.dumps(v, ensure_ascii=False, separators=(",",":"), allow_nan=False).encode("utf-8")
def no_duplicates(pairs):
    out={}
    for k,v in pairs:
        require(k not in out,"duplicate JSON member")
        out[k]=v
    return out
def json_validate(kind, raw):
    require(1 <= len(raw) <= (16384 if kind=="discovery" else 65536), "document bytes")
    def refuse_float(value): raise ValueError("noninteger JSON number")
    value=json.loads(raw.decode("utf-8","strict"), object_pairs_hook=no_duplicates, parse_float=refuse_float, parse_constant=refuse_float)
    def depth(v,n=0):
        require(n <= 16,"nesting")
        if isinstance(v,dict):
            for x in v.values(): depth(x,n+1)
        elif isinstance(v,list):
            for x in v: depth(x,n+1)
    depth(value)
    require(canonical(value)==raw,"canonical bytes")
    RULES[kind](value)
    if kind == "repository":
        ids=[k["keyId"] for k in value["catalogueKeys"]]
        keys=[k["publicKeyEd25519"] for k in value["catalogueKeys"]]
        require(ids == sorted(ids) and len(set(ids)) == len(ids),"catalogue key IDs")
        require(len(set(keys)) == len(keys),"catalogue public keys")
    if kind == "discovery":
        require(all(e["appId"] == value["appId"] for e in value["entries"]),"discovery app equality")
        tuples=[tuple(e.values()) for e in value["entries"]]
        require(tuples == sorted(tuples),"discovery order")
    return value

def bind_repository_app(host, repository, discovery, catalogue, enrollment):
    # This is only the definition's cross-object equality predicate, not native admission.
    expected=(enrollment["repositoryId"],enrollment["appId"])
    primitive("h32")(expected[0])
    primitive("app")(expected[1])
    for observed in [host,repository,discovery,catalogue]:
        require((observed["repositoryId"],observed["appId"]) == expected,"repository app continuity")

EVIDENCE_KINDS=["provenance","transition","signature-u1","signature-u2","metadata-admission","repository-policy","recovery-policy","publisher-policy","issuer-policy","discovery-policy","recovery-directive"]
def evidence_location(ref):
    obj([("kind",lambda v:require(v in EVIDENCE_KINDS,"evidence kind")),("sha256","h32"),("bytes",lambda v:require(type(v) is int and 1 <= v <= 65536,"evidence bytes"))])(ref)
    if ref["kind"] in ["signature-u1","signature-u2"]:
        require(ref["bytes"]==64,"signature bytes")
    return "evidence/objects/"+ref["kind"]+"/"+ref["sha256"]

def case(name, fn, passes=True):
    try:
        fn()
        actual=True
    except (ValueError, UnicodeError, OverflowError, struct.error, IndexError):
        actual=False
    require(actual==passes,"unexpected result: "+name)
    TESTS.append({"name":name,"expected":"accept" if passes else "refuse","result":"PASS"})

def build():
    # Public deterministic fixture seeds. They must never become production authority.
    keys=[Ed25519PrivateKey.from_private_bytes(bytes([n])*32) for n in [1,2,3]]
    pubs=[k.public_key().public_bytes(Encoding.Raw,PublicFormat.Raw).hex() for k in keys]
    issuer={"schema":"keld.public-trust-issuer-policy/v1","chainPolicy":"windows-generic-verify-v2","rootCertificateSha256":[H("fixture root")],"leafIssuerSpkiSha256":[H("fixture issuer")]}
    policy={"schema":"keld.publisher-policy/v2","provider":PROVIDER,"profileOid":"1.3.6.1.4.1.311.97.990309390.766961637.194916062.941502583","issuerPolicySha256":hashlib.sha256(canonical(issuer)).hexdigest()}
    lp=lambda s:struct.pack(">H",len(s.encode()))+s.encode()
    scope=hashlib.sha256(b"keld.publisher.windows/v2\0"+lp(PROVIDER)+lp(policy["profileOid"])).hexdigest()
    app="dev.keld.fixture"
    profile=hashlib.sha256(b"keld.profile.identity/v2\0"+bytes.fromhex(scope)+lp(app)).hexdigest()
    entry={"identityScheme":"public-trust-profile-v2","publisherScope":scope,"appId":app,"channel":"stable","target":"windows-x64","rootPolicy":"keld-localappdata-app-v2","ownerProducer":"keld-provenance-v3"}
    discovery={"schema":"keld.windows.discovery-policy/v1","repositoryId":H("fixture repository identity"),"appId":app,"entries":[entry]}
    repo={"schema":"keld.repository-policy/v1","repositoryId":discovery["repositoryId"],"appId":app,"metadataBaseUrl":"https://updates.example.invalid/metadata/","targetsBaseUrl":"https://updates.example.invalid/targets/","bootstrapRootSha256":H("fixture root metadata"),"bootstrapRootVersion":1,"metadataProfile":1,"target":"windows-x64","catalogueKeys":[{"keyId":H("synthetic key-id grammar only"),"publicKeyEd25519":pubs[0]}],"publisherPolicySha256":hashlib.sha256(canonical(policy)).hexdigest(),"discoveryPolicySha256":hashlib.sha256(canonical(discovery)).hexdigest()}
    repo["recoveryPolicySha256"]=H("synthetic recovery policy grammar only")
    f={"appId":app,"channel":"stable","target":"windows-x64","updatePublicKeyEd25519":pubs[1],"profileOid":policy["profileOid"],"issuerPolicySha256":policy["issuerPolicySha256"],"repositoryId":repo["repositoryId"],"repositoryPolicySha256":hashlib.sha256(canonical(repo)).hexdigest(),"discoveryPolicySha256":repo["discoveryPolicySha256"]}
    package={"version":"2.0.0","manifestSha256":H("manifest"),"manifestBytes":512,"manifestSignatureSha256":H("manifest signature"),"manifestSignatureBytes":88,"transportBlake3":H("synthetic transport grammar"),"transportBytes":1000,"contentBlake3":H("synthetic content grammar"),"contentBytes":2048,"profileDigest":H("strict profile")}
    cat={"schema":"keld.windows.initial-catalogue/v2","repositoryId":repo["repositoryId"],"repositoryPolicySha256":f["repositoryPolicySha256"],"catalogueKeyId":repo["catalogueKeys"][0]["keyId"],"appId":app,"channel":"stable","target":"windows-x64","publisherPolicy":policy,"discoveryPolicySha256":repo["discoveryPolicySha256"],"host":{"sha256":H("host"),"bytes":4000},"package":package,"updatePublicKeyEd25519":pubs[2],"installIntent":{"owner":"direct","mode":"per-user-direct","principalModel":"strict-distinct-os-principals","rootPolicy":"keld-localappdata-app-v2"}}
    identity={"scheme":"public-trust-profile-v2","publisherScope":scope,"publisherPolicySha256":repo["publisherPolicySha256"]}
    source={"version":"1.0.0","contentBlake3":H("old content grammar"),"hostSha256":H("old Host"),"helperSha256":H("old helper"),"manifestSha256":H("old manifest"),"updatePublicKeyEd25519":pubs[1],"hostSpkiSha256":H("old SPKI"),"identity":identity}
    dest={"catalogueSha256":hashlib.sha256(canonical(cat)).hexdigest(),**source,"version":"2.0.0","contentBlake3":package["contentBlake3"],"hostSha256":cat["host"]["sha256"],"helperSha256":H("new helper"),"manifestSha256":package["manifestSha256"],"updatePublicKeyEd25519":pubs[2],"hostSpkiSha256":H("new SPKI")}
    tx={"schema":"keld.windows.identity-transition/v1","repositoryId":repo["repositoryId"],"appId":app,"channel":"stable","target":"windows-x64","source":source,"destination":dest,"rollback":source,"browserStore":{"mode":"retain-source","profileIdentity":profile},"installMode":"per-user-direct","principalModel":"strict-distinct-os-principals"}
    prov={"schema":"keld.install-provenance/v3","owner":"direct","protection_profile":"windows-per-user-role-rx-v1","identity":{"install_mode":"per-user-direct","app_id":app,"channel":"stable","target":"windows-x64","install_root":"C:\\Users\\Fixture\\AppData\\Local\\GYLDLAB\\Keld\\v2\\fixture","update_root":"C:\\Users\\Fixture\\AppData\\Local\\GYLDLAB\\Keld\\v2\\fixture\\updates","signing_key_id":H("synthetic key-id grammar"),"baseline":{"app_id":app,"channel":"stable","target":"windows-x64","version":"2.0.0","content_blake3":package["contentBlake3"]},"profile_digest":package["profileDigest"],"principal_model":"strict-distinct-os-principals"},"publisher":{"scheme":"public-trust-profile-v2","scope":scope,"policy":policy},"volume_guid":"\\\\?\\Volume{00000000-0000-0000-0000-000000000001}\\","repository_id":repo["repositoryId"],"repository_policy_sha256":f["repositoryPolicySha256"],"discovery_policy_sha256":repo["discoveryPolicySha256"],"browser_binding":{"kind":"fresh-v2","profile_identity":profile},"baseline_signers":{"host":{"sha256":H("new Host"),"spki_sha256":H("new SPKI")},"helper":{"sha256":H("new helper"),"spki_sha256":H("new helper SPKI")}},"recovery_bindings":[]}
    fixtures=dict(issuer=issuer,publisher=policy,repository=repo,discovery=discovery,catalogue=cat,transition=tx,provenance=prov)
    raw=encode_v2(f)
    decoded,offsets=decode_payload(raw,2)
    case("v2 normal round trip",lambda:require(decoded==f))
    old=encode_v1(f)
    case("v1 matching domain",lambda:decode_payload(old,1))
    original_v1=(b"keld.expected-app-identity/v1\0\x0fcom.example.app\x06stable\x0bwindows-x64"
                 b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f"
                 b"\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f")
    case("preserved existing source v1 literal",lambda:require(encode_v1({"appId":"com.example.app","channel":"stable","target":"windows-x64","updatePublicKeyEd25519":bytes(range(32)).hex()}) == original_v1))
    case("v1 rejects v2",lambda:decode_payload(raw,1),False)
    case("v2 rejects v1",lambda:decode_payload(old,2),False)
    low={**f,"appId":"a","channel":"a","target":"a","profileOid":"0.0"}
    high={**f,"appId":"a"*255,"channel":"c"*16,"target":"t"*64,"profileOid":"2."+"123456789."*24+"123456789.123"}
    require(len(high["profileOid"])==255)
    for name,data,size in [("minimum byte payload",encode_v2(low),206),("maximum byte payload",encode_v2(high),790)]:
        case(name,lambda data=data,size=size:require(len(data)==size and decode_payload(data,2)[0]))
    for n,pos in offsets.items():
        case("truncated before "+n,lambda pos=pos:decode_payload(raw[:pos],2),False)
    for n in ["provider","owner","installMode","principalModel","rootPolicy"]:
        changed=bytearray(raw);changed[offsets[n]]=255
        case("unsupported "+n,lambda changed=bytes(changed):decode_payload(changed,2),False)
    case("trailing byte",lambda:decode_payload(raw+b"\0",2),False)
    case("empty app",lambda:encode_v2({**f,"appId":""}),False)
    case("app limit plus one",lambda:encode_v2({**f,"appId":"a"*256}),False)
    case("channel limit plus one",lambda:encode_v2({**f,"channel":"a"*17}),False)
    case("target limit plus one",lambda:encode_v2({**f,"target":"a"*65}),False)
    for bad in ["1.03.6","1..6","3.1","1.40","2.4294967296","1.3.","1.3\n","1.+3","1.3 "]:
        case("invalid oid "+repr(bad),lambda bad=bad:oid(bad),False)
    for kind,value in fixtures.items():
        b=canonical(value)
        case(kind+" canonical shape",lambda kind=kind,b=b:json_validate(kind,b))
        case(kind+" newline refusal",lambda kind=kind,b=b:json_validate(kind,b+b"\n"),False)
        changed={**value,"unknown":1}
        case(kind+" unknown field",lambda kind=kind,changed=changed:json_validate(kind,canonical(changed)),False)
        case(kind+" duplicate field",lambda kind=kind,b=b:json_validate(kind,b'{"schema":"wrong",'+b[1:]),False)
    for bad in ["https://user@updates.example.invalid/","https://updates.example.invalid/../","https://updates.example.invalid/%2e/","http://updates.example.invalid/","https://updates.example.invalid:444/","https://updates.example.invalid/?token=1"]:
        case("forbidden URL "+bad,lambda bad=bad:json_validate("repository",canonical({**repo,"metadataBaseUrl":bad})),False)
    case("metadata profile rejects JSON true",lambda:json_validate("repository",canonical({**repo,"metadataProfile":True})),False)
    case("metadata profile rejects float",lambda:json_validate("repository",canonical({**repo,"metadataProfile":1.0})),False)
    case("repository missing recovery policy",lambda:json_validate("repository",canonical({k:v for k,v in repo.items() if k != "recoveryPolicySha256"})),False)
    case("repository missing app",lambda:json_validate("repository",canonical({k:v for k,v in repo.items() if k != "appId"})),False)
    anchor={"repositoryId":repo["repositoryId"],"appId":app}
    case("repository one-app agreement",lambda:bind_repository_app(f,repo,discovery,cat,anchor))
    for name,index in [("Host",0),("repository policy",1),("discovery",2),("catalogue",3)]:
        changed=copy.deepcopy([f,repo,discovery,cat])
        changed[index]["appId"]="dev.keld.other"
        case("repository app mismatch in "+name,lambda changed=changed:bind_repository_app(*changed,anchor),False)
    rebound=copy.deepcopy([f,repo,discovery,cat])
    for observed in rebound:
        observed["appId"]="dev.keld.other"
    case("coherent other app cannot reuse enrolled repository",lambda:bind_repository_app(*rebound,anchor),False)
    ref={"kind":"repository-policy","sha256":hashlib.sha256(canonical(repo)).hexdigest(),"bytes":len(canonical(repo))}
    case("evidence reference derives unique path",lambda:require(evidence_location(ref)=="evidence/objects/repository-policy/"+ref["sha256"]))
    case("evidence reference rejects attempt selector",lambda:evidence_location({**ref,"attemptId":H("caller attempt")}),False)
    case("evidence reference rejects path in digest",lambda:evidence_location({**ref,"sha256":"../"+ref["sha256"]}),False)
    retained=copy.deepcopy(prov)
    retained["browser_binding"]={"kind":"retained-predecessor","profile_identity":profile,"owner_record_sha256":H("owner"),"predecessor":entry}
    case("retained browser union",lambda:json_validate("provenance",canonical(retained)))
    invalid_fresh=copy.deepcopy(prov);invalid_fresh["browser_binding"]["owner_record_sha256"]=H("owner")
    case("fresh browser rejects premature owner record",lambda:json_validate("provenance",canonical(invalid_fresh)),False)
    for authorization_kind in ["planned-transition", "manual-recovery"]:
        recovered = copy.deepcopy(prov)
        recovered["recovery_bindings"] = [{"artifact":source,"prior_provenance_sha256":H("prior provenance"),"authorization":{"kind":authorization_kind,"sha256":H("authorization")},"profile_identity":profile}]
        case("recovery authorization "+authorization_kind,lambda recovered=recovered:json_validate("provenance",canonical(recovered)))
    invalid_recovery = copy.deepcopy(recovered)
    invalid_recovery["recovery_bindings"][0]["authorization"]["kind"] = "missing-signature-fallback"
    case("recovery rejects implicit fallback",lambda:json_validate("provenance",canonical(invalid_recovery)),False)
    messages={"catalogue":CAT_DOMAIN+canonical(cat),"transition":TX_DOMAIN+canonical(tx)}
    sigs={"catalogue":keys[0].sign(messages["catalogue"]),"u1":keys[1].sign(messages["transition"]),"u2":keys[2].sign(messages["transition"])}
    def verify(key,sig,msg):
        try: keys[key].public_key().verify(sig,msg)
        except Exception as e: raise ValueError("signature") from e
    case("catalogue domain signature",lambda:verify(0,sigs["catalogue"],messages["catalogue"]))
    case("transition U1 signature",lambda:verify(1,sigs["u1"],messages["transition"]))
    case("transition U2 signature",lambda:verify(2,sigs["u2"],messages["transition"]))
    case("catalogue signature cannot authorize transition",lambda:verify(0,sigs["catalogue"],TX_DOMAIN+canonical(cat)),False)
    case("U1 signature cannot satisfy U2 proof",lambda:verify(2,sigs["u1"],messages["transition"]),False)
    case("literal transition mutation",lambda:verify(1,sigs["u1"],messages["transition"]+b" "),False)
    case("renewal scope independent of SPKI",lambda:require(source["hostSpkiSha256"]!=dest["hostSpkiSha256"] and source["identity"]["publisherScope"]==dest["identity"]["publisherScope"]))
    vector={"schema":"keld.definition-identity-vectors/v1","productPin":"7ff41173d28d58ae61ccbb3382d055c8ef9c9c97","qualification":"definition-only; synthetic fixtures; no native or product execution","fields":f,"payloadV1Hex":old.hex(),"payloadV2Hex":raw.hex(),"payloadV2Sha256":hashlib.sha256(raw).hexdigest(),"minimumPayloadHex":encode_v2(low).hex(),"maximumPayloadHex":encode_v2(high).hex(),"stablePublisherScope":scope,"browserProfileIdentity":profile,"fixturePublicKeys":pubs,"literalDocuments":{k:canonical(v).decode() for k,v in fixtures.items()},"signatureMessagesSha256":{k:hashlib.sha256(v).hexdigest() for k,v in messages.items()},"signatureHex":{k:v.hex() for k,v in sigs.items()},"limitations":["BLAKE3 fields are synthetic grammar fixtures, not computed product identities.","Certificate, metadata, package, native-path, issuer, and artifact authenticity are not exercised.","Ed25519 domain checks use cryptography; product verify_strict and weak-key qualification remain separate.","Boundary codec vectors are syntactic; app/channel/target provider eligibility requires owning semantic validation."]}
    (ROOT/"identity-vectors.json").write_text(json.dumps(vector,indent=2)+"\n", encoding="utf-8", newline="\n")
    report={"status":"PASS","suite":"definition-only","tests":len(TESTS),"passed":len(TESTS),"failed":0,"checks":TESTS,"limitations":vector["limitations"]}
    (ROOT/"identity-validation.json").write_text(json.dumps(report,indent=2)+"\n", encoding="utf-8", newline="\n")
    print(json.dumps({k:report[k] for k in ["status","suite","tests","passed","failed"]}))

if __name__ == "__main__": build()
