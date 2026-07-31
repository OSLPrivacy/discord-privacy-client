from pathlib import Path
import unittest

ROOT=Path(__file__).parent

class PublicPairingExchangeTests(unittest.TestCase):
    def test_exchange_is_fixed_path_public_only_and_managed_identity(self):
        source=(ROOT/'exchange-whatsapp-public-pairing.ps1').read_text()
        self.assertIn('whatsapp-qa-offer.v1.json',source)
        self.assertIn('whatsapp-qa-peer-offer.v1.json',source)
        self.assertIn('169.254.169.254/metadata/identity',source)
        self.assertIn("PrivateIdentityRead=$false",source)
        self.assertIn("WhatsAppPrivateStorageRead=$false",source)
        self.assertNotRegex(source,r'(?i)(sas|accountkey|password|setforegroundwindow|sendkeys)')

    def test_consume_binds_hash_and_restarts_only_exact_osl(self):
        source=(ROOT/'exchange-whatsapp-public-pairing.ps1').read_text()
        self.assertIn("Get-Sha256Bytes $bytes) -cne $ExpectedPeerOfferSha256",source)
        self.assertIn("Name = 'OSL Privacy.exe'",source)
        self.assertIn('OslProcessRestarted=$true',source)
        self.assertNotIn("Terminate -Name",source)

    def test_controller_requires_semantic_receipts_from_exact_pair(self):
        source=(ROOT/'whatsapp-public-pairing-orchestrator.py').read_text()
        self.assertIn('vms=deploy.ALLOWED_VMS',source)
        self.assertIn("result.get('WhatsAppProcessSetUnchanged') is not True",source)
        self.assertIn("result.get('WindowForegrounded') is not False",source)
        self.assertNotIn('print(result',source)

if __name__=='__main__': unittest.main()
