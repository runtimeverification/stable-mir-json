#!/usr/bin/env python3

from collections import defaultdict
from functools import reduce
import glob
import json
import os
import re
import sys
from pathlib import Path

DEBUG = os.getenv('DEBUG', False)
RUSTC_PAT = '\.smir\.json'
CARGO_PAT = '-[a-z0-9]+\.smir\.json'
NUM_PREFIX_RE = re.compile('^[1-9][0-9]*')
SYM_SUFFIX_RE = re.compile('^17h[a-f0-9]{16}E$')
ESC_RE = re.compile('\.\.|\$(SP|BP|RF|LT|GT|LP|RP|C|u[0-9a-f]{1,4})\$')

def num_prefix(s):
  num_match = NUM_PREFIX_RE.match(s)
  if not num_match: raise ValueError("Rust symbol had unexepcted format")
  return int(num_match[0]), len(num_match[0])

def unescape_helper(match_obj):
  if match_obj[0][0] == '.': return '::'
  esc_sym = match_obj[1]
  match esc_sym:
    case "SP": return '@'
    case "BP": return '*'
    case "RF": return '&'
    case "LT": return '<'
    case "GT": return '>'
    case "LP": return '('
    case "RP": return ')'
    case "C" : return ','
    case str(esc): return chr(int(esc[1:], 16))
  raise ValueError(f"Unexpected match object: {match_obj}")

def unescape(sym_grp):
  return ESC_RE.sub(unescape_helper, sym_grp)

def demangle(sym_name):
  if not ( sym_name[:3] == '_ZN' and SYM_SUFFIX_RE.match(sym_name[-20:]) ):
    return sym_name, None
  base = sym_name[3:-20]
  sym_hash = sym_name[-17:-1]
  groups = []
  while len(base) > 0:
    grp_sz, skip_cnt = num_prefix(base)
    if grp_sz + skip_cnt > len(base): raise ValueError("Rust symbol group has unexpected length")
    grp = base[skip_cnt:skip_cnt+grp_sz]
    if grp[0] == '_': grp = grp[1:]
    groups.append(unescape(grp))
    base = base[skip_cnt+grp_sz:]
  return tuple(groups), sym_hash

def matching_file(crate_name, path):
  if re.match(RUSTC_PAT, path.name) or re.match(CARGO_PAT, path.name):
      return path
  else:
      return None

def load_files(name, files):
  data = {}
  path_match = None
  # use files
  if files:
    for file in files:
        path = Path(file)
        current_match = matching_file(name, path)
        if current_match:
            if path_match: raise ValueError(f"Two matching files found: '{current_match}' and '{path_match}'")
            path_match = current_match
        data[file] = json.loads(path.read_text())
  # perform glob
  else:
    pat = f'{name}*.smir.json'
    candidates = glob.glob(pat, root_dir=Path.cwd())
    if len(candidates) != 1: raise ValueError(f"Non-unqiue matching candidate(s) for {name} found: {candidates}")
    path_match = matching_file(name, candidates)
    data[path_match.name] = json.load(path_match)
  return data

def run(name, files):
  crate_data = load_files(name, files)
  present_items = defaultdict(set)
  link_missing = defaultdict(lambda: defaultdict(set))
  present_items_mangled = set()
  link_foreign = set()
  for crate_name, crate_datum in crate_data.items():
    for item in crate_datum['items']:
      sym_name = item['symbol_name']
      name, sym_hash = demangle(sym_name)
      present_items[name].add(sym_hash)
      present_items_mangled.add(sym_name)
  for crate_name, crate_datum in crate_data.items():
    for (ty,func) in crate_datum['functions']:
      func_name = func.get('NormalSym', None)
      if func_name:
        if not isinstance(func_name, str): raise ValueError("Ill-formed functions table")
        foreign_func = func_name[0:3] != "_ZN"
        if foreign_func:
          link_foreign.add(func_name)
        else:
          demangled_func_name,sym_hash = demangle(func_name)
          if sym_hash not in present_items[demangled_func_name]:
            link_missing[demangled_func_name][crate_name].add(sym_hash)
  present_items_size = reduce(lambda x,y: x+len(y), present_items.values(), 0)
  if len(present_items_mangled) != present_items_size:
    raise ValueError("De-mangling process failed")
  for item in sorted(link_foreign): print(item)
  for k in sorted(link_missing.keys()):
    for crate in link_missing[k]:
      crate_name = Path(crate).stem
      if crate_name.endswith('.smir'): crate_name = crate_name[:-5]
      print(f'{crate_name}::{"::".join(k)}->{sorted(link_missing[k][crate])}')

if __name__ == "__main__":
    if len(sys.argv) < 2: print(f"USAGE: {sys.argv[0]} crate-name [json file list]\n\nPrint mono-items whose bodies were not found in compiled crate metadata")
    name = sys.argv[1]
    files = sys.argv[2:] if len(sys.argv) > 2 else None
    re_name = re.escape(name)
    RUSTC_PAT = re_name + RUSTC_PAT
    CARGO_PAT = re_name + CARGO_PAT
    try:
        run(name, files)
    except Exception as e:
        if DEBUG: raise e
        print(f"Error: {e}")
        sys.exit(1)
