# Governance

Argentum is a single-maintainer open-source project run by AG064.

## Roles and responsibilities

| Role | Who | Responsibilities |
| --- | --- | --- |
| Maintainer | AG064 ([@AG064](https://github.com/AG064), contact@ag064.eu) | Sets the direction and the roadmap, reviews and merges changes, makes releases, handles security reports under [SECURITY.md](SECURITY.md), enforces the [code of conduct](CODE_OF_CONDUCT.md), and holds the project's accounts and signing keys. |
| Contributor | Anyone who sends an issue or pull request | Follows [CONTRIBUTING.md](CONTRIBUTING.md) and the code of conduct, and accepts the [contributor agreement](CONTRIBUTOR_AGREEMENT.md) for code contributions. |
| Security reporter | Anyone who reports a vulnerability | Reports privately as described in SECURITY.md. |

## How decisions are made

- The maintainer decides. Larger changes start as an issue, so the approach can be
  discussed in public before work begins.
- Every change, including the maintainer's own, reaches the `development` branch
  through a pull request; the branch ruleset blocks direct pushes. Pull requests
  are merged when they build, pass CI and follow CONTRIBUTING.md. While the
  project has one maintainer, the maintainer may merge their own pull request
  without a second approval. The maintainer gives reasons when a change is
  declined.
- Security fixes may be prepared in private and published with the release that
  contains them.
- The roadmap in [ROADMAP.md](ROADMAP.md) records what is planned. Supporters may
  vote on the order of planned work; votes do not add commitments.

## Access to sensitive resources

| Resource | Who has access |
| --- | --- |
| GitHub repository AG064/argentum (admin) | Maintainer, with two-factor authentication |
| GitHub Actions secrets, including the Android release signing key | Maintainer |
| Offline backup of the signing key and recovery codes | Maintainer and the designated successor |
| ag064.eu website and domain | Maintainer |
| OpenSSF Best Practices entry | Maintainer |

Before anyone is given write, admin or secret access, the maintainer reviews their
contribution history in this project and confirms who they are. Access is reviewed
again when a person changes or leaves a role, and secrets are rotated when they
leave.

## Continuity

The project must be able to go on if the maintainer becomes unavailable.

- The maintainer has named a successor in GitHub's successor settings. If the
  maintainer can no longer act, the successor can take over the repository, so
  issues can be handled, changes accepted and releases made.
- The successor can reach the offline backup of the release signing key and the
  recovery codes needed to make a signed release.
- Releases follow [docs/RELEASING.md](docs/RELEASING.md), so a new maintainer can
  publish one with the documented steps.
- The source code is public under the AGPL-3.0, so anyone can continue the work in a
  fork.
