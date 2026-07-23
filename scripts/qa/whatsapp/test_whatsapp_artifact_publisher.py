from pathlib import Path
import unittest

ROOT=Path(__file__).parent

class ArtifactPublisherTests(unittest.TestCase):
    def test_chunk_stage_is_bounded_hashed_and_idempotent(self):
        source=(ROOT/'stage-whatsapp-artifact-chunk.ps1').read_text()
        self.assertIn('ChunkBase64.Length -gt 700000',source)
        self.assertIn('SHA256]::Create()',source)
        self.assertNotIn('HashData',source)
        self.assertNotIn('ToHexString',source)
        self.assertIn("'alreadyStaged'",source)
        self.assertNotIn('Invoke-RestMethod',source)

    def test_finalize_uses_only_managed_identity_and_fixed_blob_host(self):
        source=(ROOT/'finalize-whatsapp-artifact-publish.ps1').read_text()
        self.assertIn('169.254.169.254/metadata/identity',source)
        self.assertIn('osltestartifactsa7d5.blob.core.windows.net',source)
        self.assertIn("'x-ms-blob-type'='BlockBlob'",source)
        self.assertIn("'x-ms-date'",source)
        self.assertIn('TokenReturned=$false',source)
        self.assertIn('finally{$token=$null}',source)
        self.assertNotRegex(source,r'(?i)(sas|accountkey|password|keyvault secret show)')

    def test_controller_uses_parameter_file_and_redacts_failures(self):
        source=(ROOT/'whatsapp-artifact-publisher.py').read_text()
        self.assertIn('"--body", f"@{request_path}"',source)
        self.assertIn('"& {\\n" + script.read_text',source)
        self.assertNotIn('"parameters": parameters',source)
        self.assertIn('ThreadPoolExecutor(max_workers=args.workers)',source)
        self.assertIn('if not args.finalize_only:',source)
        self.assertIn('raise PublishError(f"{phase}: Azure RunCommand failed closed")',source)
        self.assertNotIn('print(chunk',source)
        self.assertNotIn('stdout=PIPE',source)

if __name__=='__main__': unittest.main()
