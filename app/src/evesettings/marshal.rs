//! Reader and writer for the object stream EVE keeps its settings files in.
//!
//! A file decodes to a tree of [`Node`]s that remembers how each value was written, so a file that
//! was not edited encodes back to the same bytes. Edits replace whole nodes; see [`Doc::own`].

use std::borrow::Cow;

const MAGIC: u8 = 0x7E;
const SHARED: u8 = 0x40;
const MAX_DEPTH: u32 = 200;

mod op {
    pub const NONE: u8 = 0x01;
    pub const TOKEN: u8 = 0x02;
    pub const I64: u8 = 0x03;
    pub const I32: u8 = 0x04;
    pub const I16: u8 = 0x05;
    pub const I8: u8 = 0x06;
    pub const MINUS_ONE: u8 = 0x07;
    pub const ZERO: u8 = 0x08;
    pub const ONE: u8 = 0x09;
    pub const F64: u8 = 0x0A;
    pub const F64_ZERO: u8 = 0x0B;
    pub const BUFFER: u8 = 0x0D;
    pub const STR_EMPTY: u8 = 0x0E;
    pub const STR_CHAR: u8 = 0x0F;
    pub const STR_SHORT: u8 = 0x10;
    pub const STR_TABLE: u8 = 0x11;
    pub const UCS2: u8 = 0x12;
    pub const STR_LONG: u8 = 0x13;
    pub const TUPLE: u8 = 0x14;
    pub const LIST: u8 = 0x15;
    pub const DICT: u8 = 0x16;
    pub const INSTANCE: u8 = 0x17;
    pub const REF: u8 = 0x1B;
    pub const TRUE: u8 = 0x1F;
    pub const FALSE: u8 = 0x20;
    pub const REDUCE: u8 = 0x22;
    pub const NEWOBJ: u8 = 0x23;
    pub const TUPLE0: u8 = 0x24;
    pub const TUPLE1: u8 = 0x25;
    pub const LIST0: u8 = 0x26;
    pub const LIST1: u8 = 0x27;
    pub const UCS2_EMPTY: u8 = 0x28;
    pub const UCS2_CHAR: u8 = 0x29;
    pub const TUPLE2: u8 = 0x2C;
    pub const MARK: u8 = 0x2D;
    pub const UTF8: u8 = 0x2E;
    pub const LONG: u8 = 0x2F;
}

/// The strings the stream may name by index instead of spelling out. Only the entries the settings
/// files were seen to use are filled in; an index outside them still round-trips, it just has no
/// text.
fn table_text(i: u8) -> Option<&'static str> {
    Some(match i {
        3 => "age",
        9 => "capacity",
        10 => "categoryID",
        11 => "character",
        12 => "characterID",
        13 => "characterName",
        15 => "charID",
        21 => "corporationID",
        22 => "createDateTime",
        24 => "description",
        29 => "flag",
        33 => "gender",
        34 => "graphicID",
        35 => "groupID",
        36 => "header",
        39 => "itemID",
        40 => "items",
        41 => "jumps",
        42 => "line",
        43 => "lines",
        44 => "locationID",
        45 => "locationName",
        51 => "modules",
        52 => "name",
        63 => "ownerID",
        64 => "ownerName",
        65 => "quantity",
        66 => "raceID",
        68 => "securityStatus",
        71 => "singleton",
        74 => "typeID",
        75 => "used",
        76 => "userID",
        112 => "address",
        113 => "alert",
        114 => "allianceID",
        115 => "allianceid",
        116 => "bid",
        117 => "bookmark",
        118 => "bounty",
        119 => "channel",
        120 => "charid",
        121 => "constellationid",
        122 => "corpID",
        123 => "corpid",
        124 => "corprole",
        125 => "damage",
        126 => "duration",
        127 => "effects.Laser",
        131 => "issued",
        133 => "languageID",
        134 => "locationid",
        137 => "minVolume",
        138 => "orderID",
        139 => "price",
        140 => "range",
        141 => "regionID",
        142 => "regionid",
        144 => "role",
        149 => "shipid",
        150 => "sn",
        151 => "solarSystemID",
        152 => "solarsystemid",
        153 => "solarsystemid2",
        154 => "source",
        156 => "stationID",
        157 => "stationid",
        158 => "target",
        160 => "userid",
        163 => "weapon",
        _ => return None,
    })
}

fn table_index(text: &str) -> Option<u8> {
    (0..=u8::MAX).find(|i| table_text(*i) == Some(text))
}

#[derive(Clone, Debug)]
pub struct Node {
    pub v: Value,
    /// Shared slot this node defines, 0 for none.
    slot: u32,
    /// The opcode it was read with, 0 for a node made here.
    op: u8,
}

/// By value: how a node was spelled in the stream is not part of what it says.
impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.v == other.v
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reduce {
    newobj: bool,
    pub head: Node,
    pub list: Vec<Node>,
    pub dict: Vec<(Node, Node)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    None,
    Bool(bool),
    Int(i64),
    /// An integer wider than 64 bits, little-endian two's complement.
    Big(Vec<u8>),
    Float(f64),
    Token(Vec<u8>),
    Str(Vec<u8>),
    Table(u8),
    Uni(String),
    Tuple(Vec<Node>),
    List(Vec<Node>),
    /// Key first here; the stream writes the value first.
    Dict(Vec<(Node, Node)>),
    Instance(Box<(Node, Node)>),
    Reduce(Box<Reduce>),
    Ref(u32),
}

impl Node {
    pub fn new(v: Value) -> Self {
        Self { v, slot: 0, op: 0 }
    }

    pub fn none() -> Self {
        Self::new(Value::None)
    }

    pub fn bool(b: bool) -> Self {
        Self::new(Value::Bool(b))
    }

    pub fn int(i: i64) -> Self {
        Self::new(Value::Int(i))
    }

    pub fn float(f: f64) -> Self {
        Self::new(Value::Float(f))
    }

    /// ASCII becomes a byte string, as the client writes its own identifiers; anything else a
    /// unicode string.
    pub fn text(s: &str) -> Self {
        if s.is_ascii() {
            Self::new(Value::Str(s.as_bytes().to_vec()))
        } else {
            Self::new(Value::Uni(s.to_owned()))
        }
    }

    pub fn tuple(items: Vec<Node>) -> Self {
        Self::new(Value::Tuple(items))
    }

    pub fn list(items: Vec<Node>) -> Self {
        Self::new(Value::List(items))
    }

    pub fn dict(pairs: Vec<(Node, Node)>) -> Self {
        Self::new(Value::Dict(pairs))
    }

    pub fn instance(class: &str, state: Node) -> Self {
        Self::new(Value::Instance(Box::new((Node::text(class), state))))
    }

    /// A Python `set`, which the stream carries as a call to its constructor.
    pub fn set(items: Vec<Node>) -> Self {
        Self::new(Value::Reduce(Box::new(Reduce {
            newobj: false,
            head: Node::tuple(vec![
                Node::new(Value::Token(SET_TOKEN.to_vec())),
                Node::tuple(vec![Node::list(items)]),
            ]),
            list: Vec::new(),
            dict: Vec::new(),
        })))
    }
}

const SET_TOKEN: &[u8] = b"__builtin__.set";

#[derive(Clone, Debug, PartialEq)]
pub struct Doc {
    pub root: Node,
    /// Every shared value as first read, by slot - 1. A reference whose definition was edited away
    /// is written out from here.
    shared: Vec<Option<Node>>,
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
    end: usize,
    map: Vec<u32>,
    next: usize,
    shared: Vec<Option<Node>>,
}

type R<T> = Result<T, String>;

impl Reader<'_> {
    fn u8(&mut self) -> R<u8> {
        if self.p >= self.end {
            return Err("truncated".into());
        }
        self.p += 1;
        Ok(self.b[self.p - 1])
    }

    fn take(&mut self, n: usize) -> R<&[u8]> {
        if n > self.end - self.p {
            return Err("truncated".into());
        }
        self.p += n;
        Ok(&self.b[self.p - n..self.p])
    }

    fn size(&mut self) -> R<usize> {
        let b = self.u8()?;
        if b != 0xFF {
            return Ok(b as usize);
        }
        let raw = self.take(4)?;
        Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize)
    }

    fn reserve(&mut self) -> R<u32> {
        let slot = *self.map.get(self.next).ok_or("more shared values than declared")?;
        self.next += 1;
        if slot == 0 || slot as usize > self.shared.len() {
            return Err("shared slot out of range".into());
        }
        Ok(slot)
    }

    fn seq(&mut self, n: usize, depth: u32) -> R<Vec<Node>> {
        let mut out = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            out.push(self.node(depth)?);
        }
        Ok(out)
    }

    fn node(&mut self, depth: u32) -> R<Node> {
        if depth > MAX_DEPTH {
            return Err("nested too deep".into());
        }
        let raw = self.u8()?;
        let o = raw & 0x3F;
        // A container takes its slot before its children, which may take slots of their own.
        let slot = if raw & SHARED != 0 { self.reserve()? } else { 0 };
        let d = depth + 1;
        let v = match o {
            op::NONE => Value::None,
            op::TOKEN => {
                let n = self.size()?;
                Value::Token(self.take(n)?.to_vec())
            }
            op::I64 => Value::Int(i64::from_le_bytes(self.take(8)?.try_into().unwrap())),
            op::I32 => Value::Int(i32::from_le_bytes(self.take(4)?.try_into().unwrap()) as i64),
            op::I16 => Value::Int(i16::from_le_bytes(self.take(2)?.try_into().unwrap()) as i64),
            op::I8 => Value::Int(self.u8()? as i8 as i64),
            op::MINUS_ONE => Value::Int(-1),
            op::ZERO => Value::Int(0),
            op::ONE => Value::Int(1),
            op::F64 => Value::Float(f64::from_le_bytes(self.take(8)?.try_into().unwrap())),
            op::F64_ZERO => Value::Float(0.0),
            op::BUFFER | op::STR_SHORT | op::STR_LONG => {
                let n = self.size()?;
                Value::Str(self.take(n)?.to_vec())
            }
            op::STR_EMPTY => Value::Str(Vec::new()),
            op::STR_CHAR => Value::Str(vec![self.u8()?]),
            op::STR_TABLE => Value::Table(self.u8()?),
            op::UCS2 => {
                let n = self.size()?;
                let units: Vec<u16> =
                    self.take(n * 2)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                Value::Uni(String::from_utf16(&units).map_err(|_| "bad UTF-16 text")?)
            }
            op::UCS2_EMPTY => Value::Uni(String::new()),
            op::UCS2_CHAR => {
                let raw = self.take(2)?;
                let unit = u16::from_le_bytes([raw[0], raw[1]]);
                Value::Uni(String::from_utf16(&[unit]).map_err(|_| "bad UTF-16 text")?)
            }
            op::UTF8 => {
                let n = self.size()?;
                Value::Uni(String::from_utf8(self.take(n)?.to_vec()).map_err(|_| "bad UTF-8 text")?)
            }
            op::LONG => {
                let n = self.size()?;
                let raw = self.take(n)?;
                if n <= 8 {
                    let mut buf = if raw.last().is_some_and(|b| b & 0x80 != 0) { [0xFF; 8] } else { [0; 8] };
                    buf[..n].copy_from_slice(raw);
                    Value::Int(i64::from_le_bytes(buf))
                } else {
                    Value::Big(raw.to_vec())
                }
            }
            op::TUPLE => {
                let n = self.size()?;
                Value::Tuple(self.seq(n, d)?)
            }
            op::TUPLE0 => Value::Tuple(Vec::new()),
            op::TUPLE1 => Value::Tuple(self.seq(1, d)?),
            op::TUPLE2 => Value::Tuple(self.seq(2, d)?),
            op::LIST => {
                let n = self.size()?;
                Value::List(self.seq(n, d)?)
            }
            op::LIST0 => Value::List(Vec::new()),
            op::LIST1 => Value::List(self.seq(1, d)?),
            op::DICT => {
                let n = self.size()?;
                let mut pairs = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    let value = self.node(d)?;
                    let key = self.node(d)?;
                    pairs.push((key, value));
                }
                Value::Dict(pairs)
            }
            op::INSTANCE => {
                let class = self.node(d)?;
                let state = self.node(d)?;
                Value::Instance(Box::new((class, state)))
            }
            op::REF => {
                let slot = self.size()?;
                if slot == 0 || slot > self.shared.len() {
                    return Err("reference out of range".into());
                }
                Value::Ref(slot as u32)
            }
            op::TRUE => Value::Bool(true),
            op::FALSE => Value::Bool(false),
            op::REDUCE | op::NEWOBJ => {
                let head = self.node(d)?;
                let mut list = Vec::new();
                while self.b.get(self.p) != Some(&op::MARK) {
                    list.push(self.node(d)?);
                }
                self.p += 1;
                let mut dict = Vec::new();
                while self.b.get(self.p) != Some(&op::MARK) {
                    let key = self.node(d)?;
                    let value = self.node(d)?;
                    dict.push((key, value));
                }
                self.p += 1;
                Value::Reduce(Box::new(Reduce { newobj: o == op::NEWOBJ, head, list, dict }))
            }
            other => return Err(format!("unsupported opcode {other:#04x}")),
        };
        let node = Node { v, slot, op: o };
        if slot != 0 {
            self.shared[slot as usize - 1] = Some(node.clone());
        }
        Ok(node)
    }
}

struct Writer<'a> {
    out: Vec<u8>,
    shared: &'a [Option<Node>],
    defined: Vec<bool>,
    order: Vec<u32>,
    /// New slot numbers by old slot, on the second pass.
    renumber: Option<Vec<u32>>,
}

fn int_fits(o: u8, v: i64) -> bool {
    match o {
        op::MINUS_ONE => v == -1,
        op::ZERO => v == 0,
        op::ONE => v == 1,
        op::I8 => i8::try_from(v).is_ok(),
        op::I16 => i16::try_from(v).is_ok(),
        op::I32 => i32::try_from(v).is_ok(),
        op::I64 | op::LONG => true,
        _ => false,
    }
}

/// The shortest two's complement form, which is how the client writes a long.
fn long_bytes(v: i64) -> Vec<u8> {
    if v == 0 {
        return Vec::new();
    }
    let mut raw = v.to_le_bytes().to_vec();
    while raw.len() > 1 {
        let (last, prev) = (raw[raw.len() - 1], raw[raw.len() - 2]);
        let redundant = (last == 0x00 && prev & 0x80 == 0) || (last == 0xFF && prev & 0x80 != 0);
        if !redundant {
            break;
        }
        raw.pop();
    }
    raw
}

impl Writer<'_> {
    fn size(&mut self, n: usize) {
        if n < 0xFF {
            self.out.push(n as u8);
        } else {
            self.out.push(0xFF);
            self.out.extend_from_slice(&(n as u32).to_le_bytes());
        }
    }

    fn sized(&mut self, o: u8, flag: u8, bytes: &[u8]) {
        self.out.push(o | flag);
        self.size(bytes.len());
        self.out.extend_from_slice(bytes);
    }

    fn node(&mut self, n: &Node) -> R<()> {
        if let Value::Ref(slot) = n.v {
            let i = slot as usize;
            if self.defined.get(i).copied().unwrap_or(false) {
                self.out.push(op::REF);
                let to = self.renumber.as_ref().map_or(slot, |m| m[i]);
                self.size(to as usize);
                return Ok(());
            }
            // The definition was edited away, or now comes later: write it here instead.
            let def = self.shared.get(i.wrapping_sub(1)).and_then(|d| d.as_ref()).ok_or("dangling reference")?;
            if matches!(def.v, Value::Ref(_)) {
                return Err("reference to a reference".into());
            }
            return self.node(def);
        }
        let mut flag = 0;
        let i = n.slot as usize;
        if i != 0 && i < self.defined.len() && !self.defined[i] {
            self.defined[i] = true;
            self.order.push(n.slot);
            flag = SHARED;
        }
        let o = n.op;
        match &n.v {
            Value::None => self.out.push(op::NONE | flag),
            Value::Bool(b) => self.out.push(if *b { op::TRUE } else { op::FALSE } | flag),
            Value::Int(v) => {
                let v = *v;
                let o = if int_fits(o, v) {
                    o
                } else {
                    match v {
                        -1 => op::MINUS_ONE,
                        0 => op::ZERO,
                        1 => op::ONE,
                        _ if i8::try_from(v).is_ok() => op::I8,
                        _ if i16::try_from(v).is_ok() => op::I16,
                        _ if i32::try_from(v).is_ok() => op::I32,
                        _ => op::LONG,
                    }
                };
                self.out.push(o | flag);
                match o {
                    op::I8 => self.out.push(v as i8 as u8),
                    op::I16 => self.out.extend_from_slice(&(v as i16).to_le_bytes()),
                    op::I32 => self.out.extend_from_slice(&(v as i32).to_le_bytes()),
                    op::I64 => self.out.extend_from_slice(&v.to_le_bytes()),
                    op::LONG => {
                        let raw = long_bytes(v);
                        self.size(raw.len());
                        self.out.extend_from_slice(&raw);
                    }
                    _ => {}
                }
            }
            Value::Big(raw) => self.sized(op::LONG, flag, raw),
            Value::Float(f) => {
                if f.to_bits() == 0 && o != op::F64 {
                    self.out.push(op::F64_ZERO | flag);
                } else {
                    self.out.push(op::F64 | flag);
                    self.out.extend_from_slice(&f.to_le_bytes());
                }
            }
            Value::Token(raw) => self.sized(op::TOKEN, flag, raw),
            Value::Str(raw) => {
                let keep = match o {
                    op::STR_EMPTY => raw.is_empty(),
                    op::STR_CHAR => raw.len() == 1,
                    op::BUFFER | op::STR_SHORT | op::STR_LONG => true,
                    _ => false,
                };
                let o = match (keep, raw.len()) {
                    (true, _) => o,
                    (false, 0) => op::STR_EMPTY,
                    (false, 1) => op::STR_CHAR,
                    (false, _) => op::STR_LONG,
                };
                match o {
                    op::STR_EMPTY => self.out.push(o | flag),
                    op::STR_CHAR => self.out.extend_from_slice(&[o | flag, raw[0]]),
                    _ => self.sized(o, flag, raw),
                }
            }
            Value::Table(i) => self.out.extend_from_slice(&[op::STR_TABLE | flag, *i]),
            Value::Uni(s) => {
                let units = || s.encode_utf16().count();
                let keep = match o {
                    op::UCS2_EMPTY => s.is_empty(),
                    op::UCS2_CHAR => units() == 1,
                    op::UCS2 | op::UTF8 => true,
                    _ => false,
                };
                let o = if keep {
                    o
                } else if s.is_empty() {
                    op::UCS2_EMPTY
                } else if units() == 1 {
                    op::UCS2_CHAR
                } else {
                    op::UTF8
                };
                match o {
                    op::UCS2_EMPTY => self.out.push(o | flag),
                    op::UCS2_CHAR => {
                        self.out.push(o | flag);
                        let unit = s.encode_utf16().next().unwrap_or(0);
                        self.out.extend_from_slice(&unit.to_le_bytes());
                    }
                    op::UCS2 => {
                        self.out.push(o | flag);
                        self.size(units());
                        for unit in s.encode_utf16() {
                            self.out.extend_from_slice(&unit.to_le_bytes());
                        }
                    }
                    _ => self.sized(op::UTF8, flag, s.as_bytes()),
                }
            }
            Value::Tuple(items) => {
                let o = match items.len() {
                    0 if o != op::TUPLE => op::TUPLE0,
                    1 if o != op::TUPLE => op::TUPLE1,
                    2 if o != op::TUPLE => op::TUPLE2,
                    _ => op::TUPLE,
                };
                self.out.push(o | flag);
                if o == op::TUPLE {
                    self.size(items.len());
                }
                for item in items {
                    self.node(item)?;
                }
            }
            Value::List(items) => {
                let o = match items.len() {
                    0 if o != op::LIST => op::LIST0,
                    1 if o != op::LIST => op::LIST1,
                    _ => op::LIST,
                };
                self.out.push(o | flag);
                if o == op::LIST {
                    self.size(items.len());
                }
                for item in items {
                    self.node(item)?;
                }
            }
            Value::Dict(pairs) => {
                self.out.push(op::DICT | flag);
                self.size(pairs.len());
                for (key, value) in pairs {
                    self.node(value)?;
                    self.node(key)?;
                }
            }
            Value::Instance(pair) => {
                self.out.push(op::INSTANCE | flag);
                self.node(&pair.0)?;
                self.node(&pair.1)?;
            }
            Value::Reduce(r) => {
                self.out.push(if r.newobj { op::NEWOBJ } else { op::REDUCE } | flag);
                self.node(&r.head)?;
                for item in &r.list {
                    self.node(item)?;
                }
                self.out.push(op::MARK);
                for (key, value) in &r.dict {
                    self.node(key)?;
                    self.node(value)?;
                }
                self.out.push(op::MARK);
            }
            Value::Ref(_) => unreachable!(),
        }
        Ok(())
    }
}

impl Doc {
    pub fn new(root: Node) -> Self {
        Self { root, shared: Vec::new() }
    }

    pub fn decode(bytes: &[u8]) -> R<Self> {
        if bytes.len() < 6 || bytes[0] != MAGIC {
            return Err("not a settings file".into());
        }
        let count = u32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as usize;
        let trailer = count.checked_mul(4).filter(|t| *t <= bytes.len() - 5).ok_or("bad shared count")?;
        let end = bytes.len() - trailer;
        let map = bytes[end..].chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        let mut r = Reader { b: bytes, p: 5, end, map, next: 0, shared: vec![None; count] };
        let root = r.node(0)?;
        if r.p != r.end {
            return Err("trailing data".into());
        }
        Ok(Self { root, shared: r.shared })
    }

    pub fn encode(&self) -> R<Vec<u8>> {
        let pass = |renumber: Option<Vec<u32>>| -> R<Writer<'_>> {
            let mut w = Writer {
                out: Vec::new(),
                shared: &self.shared,
                defined: vec![false; self.shared.len() + 1],
                order: Vec::new(),
                renumber,
            };
            w.node(&self.root)?;
            Ok(w)
        };
        let first = pass(None)?;
        let count = first.order.len();
        // Slots must run 1..=count. They still do unless an edit dropped a shared value for good.
        let mut renumber: Vec<u32> = (0..=self.shared.len() as u32).collect();
        if first.defined.iter().skip(1).any(|d| !d) {
            let mut next = 0;
            for (slot, defined) in first.defined.iter().enumerate().skip(1) {
                if *defined {
                    next += 1;
                    renumber[slot] = next;
                }
            }
        }
        let w = pass(Some(renumber.clone()))?;
        let mut out = Vec::with_capacity(w.out.len() + 5 + count * 4);
        out.push(MAGIC);
        out.extend_from_slice(&(count as u32).to_le_bytes());
        out.extend_from_slice(&w.out);
        for slot in &w.order {
            out.extend_from_slice(&renumber[*slot as usize].to_le_bytes());
        }
        Ok(out)
    }

    pub fn resolve<'a>(&'a self, n: &'a Node) -> &'a Node {
        resolve(&self.shared, n)
    }

    pub fn view<'a>(&'a self, n: &'a Node) -> View<'a> {
        View { doc: self, n: self.resolve(n) }
    }

    pub fn root(&self) -> View<'_> {
        self.view(&self.root)
    }

    /// A copy that stands alone: references followed, sharing dropped. What may be moved into
    /// another document.
    pub fn detach(&self, n: &Node) -> Node {
        detach(&self.shared, n, 0)
    }

    /// Makes `n` safe to edit in place: a reference becomes its own copy of the value, and a
    /// definition stops being one, so other places that refer to it keep what they had.
    pub fn own<'n>(shared: &[Option<Node>], n: &'n mut Node) -> &'n mut Node {
        if let Value::Ref(_) = n.v {
            *n = resolve(shared, n).clone();
        }
        n.slot = 0;
        n
    }

    pub fn parts(&mut self) -> (&mut Node, &[Option<Node>]) {
        (&mut self.root, &self.shared)
    }
}

fn resolve<'a>(shared: &'a [Option<Node>], mut n: &'a Node) -> &'a Node {
    for _ in 0..8 {
        let Value::Ref(slot) = n.v else {
            return n;
        };
        match shared.get(slot as usize - 1).and_then(|d| d.as_ref()) {
            Some(def) => n = def,
            None => return n,
        }
    }
    n
}

fn detach(shared: &[Option<Node>], n: &Node, depth: u32) -> Node {
    let n = resolve(shared, n);
    if depth > MAX_DEPTH {
        return Node::none();
    }
    let d = depth + 1;
    let seq = |items: &[Node]| items.iter().map(|i| detach(shared, i, d)).collect::<Vec<_>>();
    let pairs = |items: &[(Node, Node)]| {
        items.iter().map(|(k, v)| (detach(shared, k, d), detach(shared, v, d))).collect::<Vec<_>>()
    };
    let v = match &n.v {
        Value::Tuple(items) => Value::Tuple(seq(items)),
        Value::List(items) => Value::List(seq(items)),
        Value::Dict(items) => Value::Dict(pairs(items)),
        Value::Instance(pair) => {
            Value::Instance(Box::new((detach(shared, &pair.0, d), detach(shared, &pair.1, d))))
        }
        Value::Reduce(r) => Value::Reduce(Box::new(Reduce {
            newobj: r.newobj,
            head: detach(shared, &r.head, d),
            list: seq(&r.list),
            dict: pairs(&r.dict),
        })),
        Value::Ref(_) => Value::None,
        other => other.clone(),
    };
    Node { v, slot: 0, op: n.op }
}

/// A node with its document, so reads follow references without the caller thinking about them.
#[derive(Clone, Copy)]
pub struct View<'a> {
    doc: &'a Doc,
    n: &'a Node,
}

impl<'a> View<'a> {
    fn of(self, n: &'a Node) -> View<'a> {
        self.doc.view(n)
    }

    pub fn int(self) -> Option<i64> {
        match self.n.v {
            Value::Int(i) => Some(i),
            Value::Bool(b) => Some(b as i64),
            _ => None,
        }
    }

    pub fn num(self) -> Option<f64> {
        match self.n.v {
            Value::Float(f) => Some(f),
            _ => self.int().map(|i| i as f64),
        }
    }

    /// The client stores flags as either a bool or 0/1.
    pub fn truthy(self) -> Option<bool> {
        match self.n.v {
            Value::Bool(b) => Some(b),
            Value::Int(i) => Some(i != 0),
            Value::None => Some(false),
            _ => None,
        }
    }

    pub fn text(self) -> Option<Cow<'a, str>> {
        match &self.n.v {
            Value::Str(raw) | Value::Token(raw) => Some(String::from_utf8_lossy(raw)),
            Value::Uni(s) => Some(Cow::Borrowed(s.as_str())),
            Value::Table(i) => table_text(*i).map(Cow::Borrowed),
            _ => None,
        }
    }

    pub fn items(self) -> Vec<View<'a>> {
        match &self.n.v {
            Value::Tuple(items) | Value::List(items) => items.iter().map(|i| self.of(i)).collect(),
            Value::Reduce(r) => self.set_items(r),
            _ => Vec::new(),
        }
    }

    fn set_items(self, r: &'a Reduce) -> Vec<View<'a>> {
        let head = self.of(&r.head).items();
        match head.as_slice() {
            [token, args] if token.text().as_deref() == Some("__builtin__.set") => {
                args.items().first().map(|l| l.items()).unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    pub fn is_seq(self) -> bool {
        matches!(self.n.v, Value::Tuple(_) | Value::List(_))
    }

    pub fn pairs(self) -> Vec<(View<'a>, View<'a>)> {
        match &self.n.v {
            Value::Dict(pairs) => pairs.iter().map(|(k, v)| (self.of(k), self.of(v))).collect(),
            _ => Vec::new(),
        }
    }

    pub fn is_dict(self) -> bool {
        matches!(self.n.v, Value::Dict(_))
    }

    pub fn get(self, key: &str) -> Option<View<'a>> {
        self.pairs().into_iter().find(|(k, _)| k.text().as_deref() == Some(key)).map(|(_, v)| v)
    }

    #[cfg(test)]
    pub fn get_int(self, key: i64) -> Option<View<'a>> {
        self.pairs().into_iter().find(|(k, _)| k.int() == Some(key)).map(|(_, v)| v)
    }

    /// Class name and state of an object instance.
    pub fn instance(self) -> Option<(View<'a>, View<'a>)> {
        match &self.n.v {
            Value::Instance(pair) => Some((self.of(&pair.0), self.of(&pair.1))),
            _ => None,
        }
    }

    pub fn detach(self) -> Node {
        self.doc.detach(self.n)
    }
}

/// Text of a key node, for finding an entry in a dict being edited.
pub fn key_text<'a>(shared: &'a [Option<Node>], n: &'a Node) -> Option<Cow<'a, str>> {
    match &resolve(shared, n).v {
        Value::Str(raw) => Some(String::from_utf8_lossy(raw)),
        Value::Uni(s) => Some(Cow::Borrowed(s.as_str())),
        Value::Table(i) => table_text(*i).map(Cow::Borrowed),
        _ => None,
    }
}

pub fn key_int(shared: &[Option<Node>], n: &Node) -> Option<i64> {
    match resolve(shared, n).v {
        Value::Int(i) => Some(i),
        _ => None,
    }
}

/// A `(text, number)` tuple key, which is how the overview names a state's colour tag or
/// background.
pub fn key_pair(shared: &[Option<Node>], n: &Node) -> Option<(String, i64)> {
    match &resolve(shared, n).v {
        Value::Tuple(items) if items.len() == 2 => {
            Some((key_text(shared, &items[0])?.into_owned(), key_int(shared, &items[1])?))
        }
        _ => None,
    }
}

/// A key the client would have written from the string table, written the same way.
pub fn key_node(text: &str) -> Node {
    match table_index(text) {
        Some(i) => Node::new(Value::Table(i)),
        None => Node::text(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(shared: &[u32], body: &[u8]) -> Vec<u8> {
        let mut out = vec![MAGIC];
        out.extend_from_slice(&(shared.len() as u32).to_le_bytes());
        out.extend_from_slice(body);
        for s in shared {
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }

    /// `{"a": ["xy", <ref xy>], "b": 5}` with "xy" shared in slot 1.
    fn shared_sample() -> Vec<u8> {
        stream(
            &[1],
            &[
                op::DICT, 2,
                op::LIST, 2, op::STR_LONG | SHARED, 2, b'x', b'y', op::REF, 1,
                op::STR_CHAR, b'a',
                op::I8, 5,
                op::STR_CHAR, b'b',
            ],
        )
    }

    #[test]
    fn unedited_stream_encodes_to_the_same_bytes() {
        let bytes = shared_sample();
        let doc = Doc::decode(&bytes).unwrap();
        assert_eq!(doc.encode().unwrap(), bytes);
        let list = doc.root().get("a").unwrap().items();
        assert_eq!(list[0].text().as_deref(), Some("xy"));
        assert_eq!(list[1].text().as_deref(), Some("xy"));
        assert_eq!(doc.root().get("b").unwrap().int(), Some(5));
    }

    #[test]
    fn every_scalar_form_round_trips() {
        let body = [
            &[op::TUPLE, 25][..],
            &[op::NONE, op::TRUE, op::FALSE, op::MINUS_ONE, op::ZERO, op::ONE],
            &[op::I8, 0x80, op::I16, 0x34, 0x12, op::I32, 1, 2, 3, 4],
            &[op::I64, 1, 2, 3, 4, 5, 6, 7, 8],
            &[op::LONG, 6, 0x0F, 0xF8, 0x3C, 0x71, 0xF5, 0x00],
            &[op::LONG, 9, 1, 2, 3, 4, 5, 6, 7, 8, 9],
            &[op::F64_ZERO, op::F64, 0, 0, 0, 0, 0, 0, 0xF0, 0x3F],
            &[op::STR_EMPTY, op::STR_CHAR, b'q', op::STR_SHORT, 2, b'h', b'i', op::STR_TABLE, 52],
            &[op::UCS2_EMPTY, op::UCS2_CHAR, 0x16, 0x27, op::UCS2, 2, b'o', 0, b'k', 0],
            &[op::UTF8, 3, 0xE2, 0x9C, 0xAA],
            &[op::TUPLE0, op::LIST0, op::LIST1, op::ONE],
        ]
        .concat();
        let bytes = stream(&[], &body);
        let doc = Doc::decode(&bytes).unwrap();
        assert_eq!(doc.encode().unwrap(), bytes);
        let items = doc.root().items();
        assert_eq!(items[6].int(), Some(-128));
        assert_eq!(items[10].int(), Some(0x00F5_713C_F80F));
        assert_eq!(items[13].num(), Some(1.0));
        assert_eq!(items[17].text().as_deref(), Some("name"));
        assert_eq!(items[19].text().as_deref(), Some("✖"));
    }

    #[test]
    fn made_values_take_the_shortest_form() {
        let node = Node::tuple(vec![
            Node::int(0),
            Node::int(300),
            Node::int(134_384_497_490_989_014),
            Node::float(0.0),
            Node::text(""),
            Node::text("ab"),
            Node::text("é"),
            Node::list(vec![]),
        ]);
        let bytes = Doc::new(node).encode().unwrap();
        let back = Doc::decode(&bytes).unwrap();
        let items = back.root().items();
        assert_eq!(items[1].int(), Some(300));
        assert_eq!(items[2].int(), Some(134_384_497_490_989_014));
        assert_eq!(items[6].text().as_deref(), Some("é"));
        let ops: Vec<u8> = match &back.root.v {
            Value::Tuple(items) => items.iter().map(|n| n.op).collect(),
            _ => panic!(),
        };
        assert_eq!(
            ops,
            vec![op::ZERO, op::I16, op::LONG, op::F64_ZERO, op::STR_EMPTY, op::STR_LONG, op::UCS2_CHAR, op::LIST0]
        );
    }

    #[test]
    fn long_bytes_are_minimal_and_signed() {
        assert_eq!(long_bytes(0x7F), vec![0x7F]);
        assert_eq!(long_bytes(0x80), vec![0x80, 0x00]);
        assert_eq!(long_bytes(-1), vec![0xFF]);
        assert_eq!(long_bytes(-129), vec![0x7F, 0xFF]);
        assert_eq!(long_bytes(0x00F5_713C_F80F), vec![0x0F, 0xF8, 0x3C, 0x71, 0xF5, 0x00]);
    }

    #[test]
    fn removing_a_shared_definition_moves_it_to_the_reference() {
        let mut doc = Doc::decode(&shared_sample()).unwrap();
        let (root, shared) = doc.parts();
        let Value::Dict(pairs) = &mut Doc::own(shared, root).v else { panic!() };
        let Value::List(items) = &mut Doc::own(shared, &mut pairs[0].1).v else { panic!() };
        items.remove(0);
        let back = Doc::decode(&doc.encode().unwrap()).unwrap();
        let list = back.root().get("a").unwrap().items();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].text().as_deref(), Some("xy"));
    }

    #[test]
    fn dropping_every_use_of_a_slot_renumbers_the_rest() {
        // Two shared strings; the first one's only uses are removed.
        let bytes = stream(
            &[1, 2],
            &[
                op::LIST, 3,
                op::STR_LONG | SHARED, 2, b'a', b'a',
                op::STR_LONG | SHARED, 2, b'b', b'b',
                op::REF, 2,
            ],
        );
        let mut doc = Doc::decode(&bytes).unwrap();
        assert_eq!(doc.encode().unwrap(), bytes);
        let (root, shared) = doc.parts();
        let Value::List(items) = &mut Doc::own(shared, root).v else { panic!() };
        items.remove(0);
        let out = doc.encode().unwrap();
        assert_eq!(u32::from_le_bytes(out[1..5].try_into().unwrap()), 1);
        let back = Doc::decode(&out).unwrap();
        let texts: Vec<String> = back.root().items().iter().map(|v| v.text().unwrap().into_owned()).collect();
        assert_eq!(texts, vec!["bb", "bb"]);
    }

    #[test]
    fn replacing_a_definition_leaves_references_alone() {
        let mut doc = Doc::decode(&shared_sample()).unwrap();
        let (root, shared) = doc.parts();
        let Value::Dict(pairs) = &mut Doc::own(shared, root).v else { panic!() };
        let Value::List(items) = &mut Doc::own(shared, &mut pairs[0].1).v else { panic!() };
        items[0] = Node::text("changed");
        let back = Doc::decode(&doc.encode().unwrap()).unwrap();
        let list = back.root().get("a").unwrap().items();
        assert_eq!(list[0].text().as_deref(), Some("changed"));
        assert_eq!(list[1].text().as_deref(), Some("xy"));
    }

    #[test]
    fn detached_copies_carry_no_references() {
        let doc = Doc::decode(&shared_sample()).unwrap();
        let copy = doc.detach(&doc.root);
        let alone = Doc::new(copy);
        let list = alone.root().get("a").unwrap().items();
        assert_eq!(list[1].text().as_deref(), Some("xy"));
        assert_eq!(u32::from_le_bytes(alone.encode().unwrap()[1..5].try_into().unwrap()), 0);
    }

    #[test]
    fn sets_and_instances_read_and_write() {
        let node = Node::tuple(vec![
            Node::set(vec![Node::int(7), Node::int(9)]),
            Node::instance("utillib.KeyVal", Node::dict(vec![(Node::text("id"), Node::text("mail"))])),
        ]);
        let bytes = Doc::new(node).encode().unwrap();
        let doc = Doc::decode(&bytes).unwrap();
        let items = doc.root().items();
        let members: Vec<i64> = items[0].items().iter().filter_map(|v| v.int()).collect();
        assert_eq!(members, vec![7, 9]);
        let (class, state) = items[1].instance().unwrap();
        assert_eq!(class.text().as_deref(), Some("utillib.KeyVal"));
        assert_eq!(state.get("id").unwrap().text().as_deref(), Some("mail"));
        assert_eq!(doc.encode().unwrap(), bytes);
    }

    #[test]
    fn damaged_input_is_an_error_not_a_panic() {
        let good = shared_sample();
        assert!(Doc::decode(&[]).is_err());
        assert!(Doc::decode(b"plain text").is_err());
        for cut in 1..good.len() {
            let _ = Doc::decode(&good[..cut]);
        }
        let mut bad = good.clone();
        bad[1] = 0xFF;
        assert!(Doc::decode(&bad).is_err());
        assert!(Doc::decode(&stream(&[], &[0x3F])).is_err());
        assert!(Doc::decode(&stream(&[], &[op::REF, 1])).is_err());
    }

    /// Run by hand against a real settings directory:
    /// `EVE_SETTINGS_SAMPLE_DIR=... cargo test --bin eve-spai marshal::tests::real_files -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_files_round_trip_byte_for_byte() {
        let dir = std::env::var("EVE_SETTINGS_SAMPLE_DIR").expect("EVE_SETTINGS_SAMPLE_DIR");
        let (mut ok, mut failed) = (0, Vec::new());
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("core_") || !name.ends_with(".dat") {
                continue;
            }
            let bytes = std::fs::read(e.path()).unwrap();
            match Doc::decode(&bytes).and_then(|d| d.encode()) {
                Ok(out) if out == bytes => ok += 1,
                Ok(out) => failed.push(format!("{} bytes in, {} out", bytes.len(), out.len())),
                Err(e) => failed.push(e),
            }
        }
        println!("{ok} files identical, {} not: {failed:?}", failed.len());
        assert!(failed.is_empty() && ok > 0);
    }
}
