#!/usr/bin/env python3
from __future__ import annotations

import argparse
import concurrent.futures
import importlib.util
import json
import re
from pathlib import Path
from typing import Any

ROOT=Path(__file__).resolve().parent
PAIR=ROOT/'exchange-whatsapp-public-pairing.ps1'
DEPLOY_CONTROLLER=ROOT/'whatsapp-deploy-orchestrator.py'
spec=importlib.util.spec_from_file_location('whatsapp_deploy_controller',DEPLOY_CONTROLLER)
if spec is None or spec.loader is None: raise RuntimeError('deployment controller unavailable')
deploy=importlib.util.module_from_spec(spec);spec.loader.exec_module(deploy)
deploy.COMMAND_NAMES[PAIR]='whatsapp-public-pairing'

def invoke(vm:str,parameters:dict[str,str])->dict[str,Any]:
    return deploy._run_command(vm,PAIR,parameters)

def main()->int:
    parser=argparse.ArgumentParser()
    parser.add_argument('--invocation',required=True)
    parser.add_argument('--receipt-dir',type=Path,default=ROOT/'receipts')
    args=parser.parse_args()
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]{7,52}',args.invocation): raise ValueError('invalid invocation ID')
    vms=deploy.ALLOWED_VMS
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
        sessions=dict(zip(vms,executor.map(deploy._discover,vms)))
    def publish(vm:str)->dict[str,Any]:
        return invoke(vm,{'ClientNumber':vm[-1],'Phase':'publish','InvocationId':args.invocation,'SessionId':str(sessions[vm])})
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
        published=dict(zip(vms,executor.map(publish,vms)))
    hashes={}
    for vm,result in published.items():
        value=result.get('OfferSha256')
        if (result.get('Schema')!='whatsapp-public-pairing/v1' or result.get('Phase')!='published'
            or not isinstance(value,str) or not re.fullmatch(r'[a-f0-9]{64}',value)
            or result.get('Terminal') is not True or result.get('ManagedIdentityOnly') is not True
            or result.get('PrivateIdentityRead') is not False or result.get('WhatsAppPrivateStorageRead') is not False
            or result.get('WhatsAppProcessSetUnchanged') is not True or result.get('WindowForegrounded') is not False):
            raise deploy.DeploymentError(f'{vm}: public offer publication failed semantic validation')
        hashes[vm]=value
    def consume(vm:str)->dict[str,Any]:
        peer=vms[1] if vm==vms[0] else vms[0]
        return invoke(vm,{'ClientNumber':vm[-1],'Phase':'consume','InvocationId':args.invocation,'SessionId':str(sessions[vm]),'ExpectedPeerOfferSha256':hashes[peer]})
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
        consumed=dict(zip(vms,executor.map(consume,vms)))
    machines=[]
    for vm,result in consumed.items():
        peer=vms[1] if vm==vms[0] else vms[0]
        if (result.get('Schema')!='whatsapp-public-pairing/v1' or result.get('Phase')!='consumedAndVerified'
            or result.get('PeerOfferSha256')!=hashes[peer] or result.get('Terminal') is not True
            or result.get('ManagedIdentityOnly') is not True or result.get('OslPublicPairingStateUpdated') is not True
            or result.get('OslProcessRestarted') is not True or result.get('PrivateIdentityRead') is not False
            or result.get('WhatsAppPrivateStorageRead') is not False or result.get('WhatsAppProcessSetUnchanged') is not True
            or result.get('WindowForegrounded') is not False):
            raise deploy.DeploymentError(f'{vm}: peer offer consumption failed semantic validation')
        machines.append({'vmName':vm,'sessionId':sessions[vm],'offerSha256':hashes[vm],
                         'peerOfferSha256':hashes[peer],'verified':True,'whatsAppProcessSetUnchanged':True})
    receipt={'schema':'whatsapp-public-pairing-controller/v1','invocationId':args.invocation,
             'status':'pairedAndVerified','machines':machines,'managedIdentityOnly':True,
             'privateIdentityRead':False,'whatsAppPrivateStorageRead':False,'windowForegrounded':False,'terminal':True}
    path=args.receipt_dir/f'{args.invocation}.json';deploy._write_receipt(path,receipt)
    print(json.dumps({'status':'pairedAndVerified','receipt':str(path)},separators=(',',':')))
    return 0

if __name__=='__main__': raise SystemExit(main())
