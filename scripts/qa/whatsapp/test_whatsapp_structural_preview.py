from pathlib import Path
import unittest

ROOT=Path(__file__).parent
class StructuralPreviewTests(unittest.TestCase):
    def test_capture_is_background_pixelated_and_secret_safe(self):
        source=(ROOT/'capture-whatsapp-structural-preview.ps1').read_text()
        self.assertIn('BitBlt',source)
        self.assertNotIn('PrintWindow',source)
        self.assertIn('NearestNeighbor',source)
        self.assertIn('ProviderTextReadable=$false',source)
        self.assertIn('WindowForegrounded=$false',source)
        self.assertNotRegex(source,r'(?i)(copyfromscreen|setforegroundwindow|sendinput|sendkeys|localstate)')

if __name__=='__main__': unittest.main()
