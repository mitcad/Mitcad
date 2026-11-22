<!-- SPDX-License-Identifier: MIT -->
# Cloud print service (design note)

Design notes for a future MitCad cloud print service: a service that
takes bodies of a design and prints them for the user. The service is not
built yet. When its own ticket is written, it links back to the tickets
named here. This note holds the requirements that other work has fixed
already.

## Components not for print services (mitcad#2214)

Some component libraries do not allow their parts to be printed as a
service. Their manifests say `non_printable`
([libraries.md](libraries.md#not-for-print-services)). The design keeps
the flag on each such part's library record, so it holds without the
library. The flag belongs to the component and its occurrences, never to
the design: the design's own bodies and the other components stay
printable.

Requirements for the upload:

1. Before it sends anything, the upload asks the model which bodies may
   go: the `print_service_check` query with the bodies or components the
   user chose (every body when nothing was chosen;
   [commands.md](../core/model-api/src/commands.md#print-service-check-mitcad2214)).
2. It sends only the bodies the answer lists as `printable`. The bodies
   of the non-printable components (`not_printable`) are left out, at
   every place where such a component is placed, also nested in a
   subassembly and also as a Build part.
3. When only non-printable bodies were chosen, the upload is refused and
   nothing is sent.
4. The user is told in words which parts were left out: the answer's
   `message` and each entry of `parts` name the component, where it is
   placed and its library, without internal ids
   ([ui-conventions.md](ui-conventions.md), "Messages and severity").
5. The flag does not change anything else. The user's own export and
   **Send to Slicer** send every body as before.
