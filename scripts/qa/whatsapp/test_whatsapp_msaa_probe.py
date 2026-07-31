from pathlib import Path
import unittest

ROOT=Path(__file__).parent
class MsaaProbeTests(unittest.TestCase):
    def test_probe_is_metadata_only_bounded_and_no_focus_or_input(self):
        source=(ROOT/'probe-whatsapp-msaa-metadata.ps1').read_text()
        self.assertIn('$total -lt $MaxNodes',source)
        self.assertIn('NamesReturned=$false',source)
        self.assertIn('ProviderContentReturned=$false',source)
        self.assertIn('WindowForegrounded=$false',source)
        self.assertIn('-WindowStyle Hidden',source)
        self.assertIn('-LogonType Interactive -RunLevel Limited',source)
        self.assertNotRegex(source,r'(?i)(setforegroundwindow|sendinput|sendkeys|whatsapp.*packages\\localstate)')

if __name__=='__main__': unittest.main()
