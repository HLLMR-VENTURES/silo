// Minimal zip writer (deflate or store, no zip64): enough for synthetic mod archives.
//   writeZip("FS25_Foo.zip", [{ name: "modDesc.xml", data: Buffer.from(xml) }]);
import fs from "node:fs";
import zlib from "node:zlib";

// A fixed timestamp (2026-09-01 12:00) so restaging gives byte-identical archives.
const DOS_TIME = (12 << 11) | (0 << 5) | 0;
const DOS_DATE = ((2026 - 1980) << 9) | (9 << 5) | 1;

/**
 * Write a zip archive.
 * @param {string} path destination file
 * @param {{ name: string, data: Buffer, store?: boolean }[]} entries members, in order
 */
export function writeZip(path, entries) {
  const fd = fs.openSync(path, "w");
  const central = [];
  let offset = 0;
  const write = (buf) => {
    fs.writeSync(fd, buf);
    offset += buf.length;
  };
  try {
    for (const e of entries) {
      const name = Buffer.from(e.name, "utf8");
      const crc = zlib.crc32(e.data) >>> 0;
      const body = e.store ? e.data : zlib.deflateRawSync(e.data, { level: 6 });
      const method = e.store ? 0 : 8;
      const local = Buffer.alloc(30);
      local.writeUInt32LE(0x04034b50, 0);
      local.writeUInt16LE(20, 4);
      local.writeUInt16LE(0x0800, 6); // UTF-8 names
      local.writeUInt16LE(method, 8);
      local.writeUInt16LE(DOS_TIME, 10);
      local.writeUInt16LE(DOS_DATE, 12);
      local.writeUInt32LE(crc, 14);
      local.writeUInt32LE(body.length, 18);
      local.writeUInt32LE(e.data.length, 22);
      local.writeUInt16LE(name.length, 26);
      local.writeUInt16LE(0, 28);
      const at = offset;
      write(local);
      write(name);
      write(body);
      const cd = Buffer.alloc(46);
      cd.writeUInt32LE(0x02014b50, 0);
      cd.writeUInt16LE(20, 4);
      cd.writeUInt16LE(20, 6);
      cd.writeUInt16LE(0x0800, 8);
      cd.writeUInt16LE(method, 10);
      cd.writeUInt16LE(DOS_TIME, 12);
      cd.writeUInt16LE(DOS_DATE, 14);
      cd.writeUInt32LE(crc, 16);
      cd.writeUInt32LE(body.length, 20);
      cd.writeUInt32LE(e.data.length, 24);
      cd.writeUInt16LE(name.length, 28);
      cd.writeUInt32LE(at, 42);
      central.push(Buffer.concat([cd, name]));
    }
    const cdStart = offset;
    for (const c of central) write(c);
    const end = Buffer.alloc(22);
    end.writeUInt32LE(0x06054b50, 0);
    end.writeUInt16LE(central.length, 8);
    end.writeUInt16LE(central.length, 10);
    end.writeUInt32LE(offset - cdStart, 12);
    end.writeUInt32LE(cdStart, 16);
    write(end);
  } finally {
    fs.closeSync(fd);
  }
}
