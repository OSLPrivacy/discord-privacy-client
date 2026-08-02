#!/usr/bin/env python3
"""Guard master revisions, authority order, and conflict closure."""
from __future__ import annotations
import argparse,json
RANK={'owner':0,'master':1,'architecture-contract':2,'checklist':3,'runtime-evidence':4,'notes-memory':5}
class SpecControlError(ValueError): pass
def validate(change:dict)->None:
 if change.get('semantic_master_edit') and (not change.get('revision_incremented') or not change.get('section_0_5_delta')):raise SpecControlError('semantic master edit needs revision increment and §0.5 delta')
 source=change.get('first_authority')
 if source not in RANK:raise SpecControlError('planned work needs a recognized first applicable authority')
 for conflict in change.get('conflicts',[]):
  if conflict.get('proposed_authority') not in RANK or conflict.get('controlling_authority') not in RANK:raise SpecControlError('conflict has unknown authority')
  if RANK[conflict['proposed_authority']] > RANK[conflict['controlling_authority']]:raise SpecControlError('lower-ranked source cannot override product intent')
  if set(conflict.get('affected_artifacts',()))-set(conflict.get('closed_artifacts',())):raise SpecControlError('conflict has unclosed affected artifact')
  if conflict.get('material_privacy_security') and not conflict.get('owner_escalated'):raise SpecControlError('material privacy/security choice requires owner escalation')
def main()->int:
 p=argparse.ArgumentParser();p.add_argument('change');a=p.parse_args()
 try:
  with open(a.change,encoding='utf-8') as f:validate(json.load(f))
 except (OSError,json.JSONDecodeError,SpecControlError) as e:print('spec-control refused:',e);return 1
 return 0
if __name__=='__main__':raise SystemExit(main())
