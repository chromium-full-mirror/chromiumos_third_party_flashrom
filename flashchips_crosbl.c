/*
 * This file is part of the flashrom project.
 *
 * Copyright 2021 Google LLC
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; version 2 of the License.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 */

#include <string.h>
#include "flash.h"

/* Cros flashrom needs to be able to automatically identify the board's flash
 * without additional input. If multiple flashchip entries are considered to be
 * suitable matches, flashrom will fail to identify the chip and be unable to
 * perform the requested operations.
 *
 * This function allows flashrom to always pick a single flashchip entry, by
 * filtering out all unwanted duplicate flashchip entries and leaving only the
 * one we want to use.
 */
bool is_chipname_duplicate(const struct flashchip *chip)
{
	/* The "GD25B128B/GD25Q128B" and "GD25Q127C/GD25Q128C" chip entries
	 * have the same vendor and model IDs.
	 *
	 * Historically cros flashrom only had the "C" entry; an initial
	 * attempt to import the "B" from upstream entry resulted in flashrom
	 * being unable to identify the flash on Atlas and Nocturne boards,
	 * causing flashrom failures documented in b/168943314.
	 */
	if(!strcmp(chip->name, "GD25B128B/GD25Q128B")) return true;

	/* MX25L128... has been split into several entries:
	 * "MX25L12805D" (b/190574697), "MX25L12833F",
	 * "MX25L12835F/MX25L12873F", "MX25L12845E/MX25L12865E" (b/332486637),
	 * and "MX25L12850F" (CB:81350),
	 * We are actually using MX25L12833F, so mark the others as duplicate.
	 */
	if(!strcmp(chip->name, "MX25L12805D") ||
	   !strcmp(chip->name, "MX25L12835F/MX25L12873F") ||
	   !strcmp(chip->name, "MX25L12845E/MX25L12865E") ||
	   !strcmp(chip->name, "MX25L12850F"))
		return true;

	/* W25Q256.V has been split into two entries: W25Q256FV and
	 * W25Q256JV_Q.
	 * Marking the latter as duplicate.
	 */
	if(!strcmp(chip->name, "W25Q256JV_Q")) return true;

	/* Mark W25Q512NW-IM (64MB) as duplicate of W25Q512NW-IM32 (32MB).
	 * This is a temporary workaround until we put 32MB part on Rex.
	 * Refer b/242958619
	 */
	if(!strcmp(chip->name, "W25Q512NW-IM")) return true;

	return false;
}
