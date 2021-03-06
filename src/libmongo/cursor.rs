/* Copyright 2013 10gen Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use std::num::*;
use std::cell::*;

use bson::encode::*;

use util::*;
use msg::*;
use index::*;
use client::Client;
use coll::Collection;

///Structure representing a cursor
pub struct Cursor {
    priv id : Option<i64>,                  // id on server (None->not yet queried, 0->closed)
    priv db: ~str,                          // name of DB associated with Cursor
    priv coll: ~str,                        // name of Collection associated with Cursor
    priv client: @Client,             // Client (+Connection) associated with Cursor
    flags : i32,                            // QUERY_FLAGs
    batch_size : i32,                       // size of batch in cursor fetch, may be modified
    query_spec : BsonDocument,              // query, may be modified
    open : bool,                            // is cursor open?
    iter_err : Option<MongoErr>,            // last error from iteration (stored in cursor)
    priv retrieved : i32,                   // number retrieved by cursor already
    priv proj_spec : Option<BsonDocument>,  // projection, does not appear to be resettable
    priv skip : i32,                        // number to skip, specify before first "next"
    priv limit : i32,                       // max to return, specify before first "next"
    priv data : ~[~BsonDocument],           // docs stored in cursor
    priv done : Cell<bool>,                 // whether [just] finished popping
}

///Iterator implementation, opens access to powerful functions like collect, advance, map, etc.
impl Iterator<~BsonDocument> for Cursor {
    /**
     * Returns pointer to next `BsonDocument`.
     *
     * Pointers passed for greater memory flexibility. Any errors
     * are stored in `Cursor`'s `iter_err` field.
     *
     * # Returns
     * `Some(~BsonDocument)` if there are more BsonDocuments,
     * `None` otherwise
     */
    pub fn next(&mut self) -> Option<~BsonDocument> {
        if self.refresh() == 0 {
            return None;
        }
        Some(self.data.pop())
    }
}

///Cursor API
impl Cursor {
    /**
     * Initialize cursor with query, projection, collection, flags,
     * and skip and limit, but don't query yet (i.e. constructed
     * cursors are empty).
     *
     * # Arguments
     * * `query` - query associated with this `Cursor`
     * * `proj` - projection of query associated with this `Cursor`
     * * `collection` - `Collection` associated with this `Cursor`;
     *                      passed for convenience
     * * `client` - `Client` associated with this `Cursor`,
     * * `flags` -  `CUR_TAILABLE`, `SLAVE_OK`, `OPLOG_REPLAY`,
     *              `NO_CUR_TIMEOUT`, `AWAIT_DATA`, `EXHAUST`,
     *              `PARTIAL`
     *
     * # Returns
     * `Cursor`
     */
    pub fn new(     query : BsonDocument,
                    proj : Option<BsonDocument>,
                    collection : &Collection,
                    client : @Client,
                    flags : i32) -> Cursor {
        Cursor {
            id: None,
            db: collection.db.clone(),
            coll: collection.name.clone(),
            client: client,
            flags: flags,
            batch_size: 0,
            query_spec: query,
            open: true,
            iter_err: None,
            retrieved: 0,
            proj_spec: proj,
            skip: 0,
            limit: 0,
            data: ~[],
            done: Cell::new(false),
        }
    }

    /**
     * Actual function used to refresh `Cursor` and iterate.
     * Any errors go into iter_err field of `Cursor`.
     *
     * # Returns
     * amount left in what's currently held by `Cursor`
     */
    fn refresh(&mut self) -> i32 {
        // clear out error
        self.iter_err = None;

        // if cursor's never been queried, query and fill data up
        if self.id.is_none() {
            let msg = mk_query(
                            self.client.inc_requestId(),
                            self.db.as_slice(),
                            self.coll.as_slice(),
                            self.flags,
                            self.skip,
                            self.batch_size,
                            self.query_spec.clone(),
                            self.proj_spec.clone());
            match self.client._send_msg(msg_to_bytes(&msg), (self.db.clone(), None), true) {
                Ok(reply) => match reply {
                    Some(r) => match r {
                        // XXX check if need start
                        OpReply { header:_, flags:_, cursor_id:id, start:_, nret:n, docs:d } => {
                            self.id = Some(id);
                            self.retrieved = n;
                            let mut d_tmp = d;
                            d_tmp.reverse();
                            self.data = d_tmp;
                            if !self.done.is_empty() { self.done.take(); }
                            self.done.put_back(false);

                            return n;
                        }
                    },
                    None => {
                        self.iter_err = Some(MongoErr::new(
                                        ~"cursor::refresh",
                                        ~"no reply",
                                        ~"received no reply from initial query"));
                        return 0;
                    }
                },
                Err(e) => {
                    self.iter_err = Some(MongoErr::new(
                                        ~"cursor::refresh",
                                        ~"sending query",
                                        fmt!("-->\n%s", e.to_str())));
                    return 0;
                }
            }

        }

        // otherwise, queried before; see if need to get_more
        if self.limit != 0 {
            // check against limit
            let diff = self.limit - self.retrieved;
            if diff > 0 { return diff; }
        }
        if self.data.len() > 0 {
            // has_next *within* cursor, so don't get_more
            return self.data.len() as i32;
        }

        // otherwise, no more within cursor, so see if can get_more
        let cur_id = self.id.clone().unwrap();
        if cur_id == 0 {
            // exhausted cursor; return
            if self.done.take() {
                // only if cursor exhausted "abnormally", set iter_err
                self.iter_err = Some(MongoErr::new(
                                        ~"cursor::refresh",
                                        ~"querying on closed cursor",
                                        ~"cannot query on closed cursor"));
            }
            self.done.put_back(true);
            return 0;
        }

        // otherwise, check if allowed to get more
        if self.retrieved >= self.limit && self.limit != 0 {
            self.iter_err = Some(MongoErr::new(
                                    ~"cursor::refresh",
                                    fmt!("cursor limit %? reached", self.limit),
                                    ~"cannot retrieve beyond limit"));
            return 0;
        }

        // otherwise, get_more
        let msg = mk_get_more(
                            self.client.inc_requestId(),
                            self.db.as_slice(),
                            self.coll.as_slice(),
                            self.batch_size,
                            cur_id);
        match self.client._send_msg(msg_to_bytes(&msg), (self.db.clone(), None), true) {
            Ok(reply) => match reply {
                Some(r) => match r {
                    // TODO check re: start
                    OpReply { header:_, flags:_, cursor_id:id, start:_, nret:n, docs:d } => {
                        // close cursor if needed
                        if id == 0 { self.close(); }

                        // also update this cursor's fields
                        self.id = Some(id);
                        self.retrieved = self.retrieved + n;
                        let mut d_tmp = d;
                        d_tmp.reverse();
                        self.data = d_tmp;
                        if !self.done.is_empty() { self.done.take(); }
                        self.done.put_back(false);

                        return n;
                    }
                },
                None => self.iter_err = Some(MongoErr::new(
                                ~"cursor::refresh",
                                ~"cursor could not refresh",
                                ~"no get_more received from server")),
            },
            Err(e) => self.iter_err = Some(e),
        }

        return 0;
    }

    /// CURSOR OPTIONS (must be specified pre-querying)
    /**
     * Skips specified amount before starting to iterate.
     *
     * # Arguments
     * * `skip` - amount to skip
     *
     * # Returns
     * () on success, `MongoErr` on failure
     *
     * # Failure Types
     * * `Cursor` already iterated over
     */
    pub fn cursor_skip(&mut self, skip: i32) -> Result<(), MongoErr> {
        if self.id.is_some() {
            return Err(MongoErr::new(
                        ~"cursor::skip",
                        ~"skipping in already queried cursor",
                        ~"must specify skip before querying cursor"));
        }

        self.skip = skip;
        Ok(())
    }

    /**
     * Limits amount to return from `Cursor`.
     *
     * # Arguments
     * * `limit` - total amount to return
     *
     * # Returns
     * () on success, `MongoErr` on failure
     *
     * # Failure Types
     * * `Cursor` already iterated over
     */
    pub fn cursor_limit(&mut self, limit: i32) -> Result<(), MongoErr> {
        if self.id.is_some() {
            return Err(MongoErr::new(
                        ~"cursor::limit",
                        ~"limiting already queried cursor",
                        ~"must specify limit before querying cursor"));
        }

        self.limit = limit;

        // also fix batch_size if needed
        if self.batch_size == 0 || self.batch_size > abs(limit as int) as i32 {
            self.batch_size = limit;
        }
        Ok(())
    }

    /// QUERY MODIFICATIONS
    /**
     * Explains the query.
     * Copies the `Cursor` and runs the query to gather information.
     * Returns query as `~BsonDocument` to ease searching for
     * specific fields, etc.
     *
     * # Returns
     * `~BsonDocument` explaining query on success, `MongoErr` on failure
     */
    pub fn explain(&mut self) -> Result<~BsonDocument, MongoErr> {
        let mut query = self.query_spec.clone();
        query.put(~"$explain", Double(1f64));
        let mut tmp_cur = Cursor::new(  query, self.proj_spec.clone(),
                                        &Collection::new(   self.db.clone(),
                                                            self.coll.clone(),
                                                            self.client),
                                        self.client,
                                        self.flags);
        tmp_cur.cursor_limit(-1);
        match tmp_cur.next() {
            Some(exp) => Ok(exp),
            None => Err(MongoErr::new(
                            ~"cursor::explain",
                            ~"no explanation",
                            ~"no explanation returned by cursor")),
        }
    }

    /**
     * Hints an index (name or fields+order) to use while querying.
     *
     * # Arguments
     * * `index` -  `MongoIndexName(name)` of index to use (if named),
     *              `MongoIndexFields(~[INDEX_TYPE])` to specify index
     *                  from fields
     *              `MongoIndex(full index)` to specify index fully,
     *                  e.g. as returned from database
     */
    pub fn hint(&mut self, index : MongoIndexSpec) {
        self.query_spec.put(~"$hint", UString(index.get_name()));
    }

    /**
     * Sorts results from `Cursor` given fields and their direction.
     *
     * # Arguments
     * * `orderby` - `NORMAL(~[(field, direction)])` where `field`s are
     *                  `~str` and `direction` are `ASC` or `DESC`
     *
     * # Returns
     * () on success, MongoErr on failure
     *
     * # Failure Types
     * * invalid sorting specification (`orderby`)
     */
    pub fn sort(&mut self, orderby : INDEX_TYPE) -> Result<(), MongoErr> {
        let mut spec = BsonDocument::new();
        match orderby {
            NORMAL(fields) => {
                for &(k,v) in fields.iter().advance {
                    spec.put(k, Int32(v as i32));
                }
            },
            _ => return Err(MongoErr::new(
                                ~"cursor::sort",
                                ~"invalid orderby specification",
                                ~"only fields and their orders allowed")),
        }
        self.query_spec.put(~"$orderby", Embedded(~spec));
        Ok(())
    }

    /**
     * Adds flags to Cursor.
     *
     * # Arguments
     * * `flags` - array of `QUERY_FLAGS` (specified above), each
     *              of which to add
     */
    pub fn add_flags(&mut self, flags : ~[QUERY_FLAG]) {
        for &f in flags.iter().advance {
            self.flags |= (f as i32);
        }
    }

    /**
     * Removes flags from Cursor.
     *
     * # Arguments
     * * `flags` - array of `QUERY_FLAGS` (specified above), each
     *              of which to remove
     */
    pub fn remove_flags(&mut self, flags : ~[QUERY_FLAG]) {
        for &f in flags.iter().advance {
            self.flags &= !(f as i32);
        }
    }

    /**
     * Modifies size of next batch to fetch on `Cursor` refresh.
     *
     * # Arguments
     * * `sz` - size of next batch to fetch on `Cursor` refresh (`QUERY`
     *          or `GET_MORE`)
     */
    pub fn batch_size(&mut self, sz : i32) {
        self.batch_size = sz;
    }

    /// OTHER USEFUL FUNCTIONS
    /**
     * Returns whether Cursor has a next `~BsonDocument`.
     * Considers the last element of a `Cursor` to be `None`, hence
     * returns `true` at edge case when `Cursor` exhausted naturally.
     */
    pub fn has_next(&mut self) -> bool {
        // return true even if right at end (normal exhaustion of cursor)
        if self.limit != 0 && self.limit >= self.retrieved { true }
        else { self.refresh() != 0 }
    }

    /**
     * Closes cursor by sending OP_KILL_CURSORS message.
     *
     * # Returns
     * () on success, `MongoErr` on failure
     */
    // TODO batch
    pub fn close(&mut self) -> Result<(), MongoErr> {
        if self.id.is_none() {
            return Err(MongoErr::new(
                            ~"cursor::close",
                            ~"no such cursor",
                            ~"cannot close cursor never iterated upon"));
        }

        // set cur_id to be 0
        let cur_id = self.id.unwrap();
        self.id = Some(0);

        // send kill_cursors message
        let kill_msg = mk_kill_cursor(
                            self.client.inc_requestId(),
                            1i32,
                            ~[cur_id]);
        let error = match self.client._send_msg(msg_to_bytes(&kill_msg), (self.db.clone(), Some(~[W_N(0)])), false) {
            Ok(reply) => match reply {
                Some(r) => Some(MongoErr::new(
                                ~"cursor::close",
                                ~"unknown error",
                                fmt!("received unexpected response %? from server",
                                    r))),
                None => None,
            },
            Err(e) => Some(e),
        };

        if error.is_none() { Ok(()) }
        else {
            self.iter_err = error.clone();
            Err(error.unwrap())
        }
    }

    /**
     * Returns whether this `Cursor` is dead, i.e. has
     * ID of 0.
     *
     * # Returns
     * whether this `Cursor` is dead and can no longer be
     * queried
     */
    pub fn is_dead(&self) -> bool {
        if self.id.is_some() { return self.id.clone().unwrap() == 0; }
        false
    }
    fn add_query_spec(&mut self, doc: &BsonDocument) {
        for &(@k, @v) in doc.fields.iter().advance {
            self.query_spec.put(k,v);
        }
    }
}

#[cfg(test)]
mod tests {
    //use super::*;
    //use bson::encode::*;
    //use util::*;
    //use coll::*;

/*    #[test]
    fn test_add_index_obj() {
        let mut doc = BsonDocument::new();
        doc.put(~"foo", Double(1f64));
        let mut cursor = Cursor::new(BsonDocument::new(), None, 0i64, 0i32, 10i32, ~[]);
        cursor.hint(SpecObj(doc));

        let mut spec = BsonDocument::new();
        let mut speci = BsonDocument::new();
        speci.put(~"foo", Double(1f64));
        spec.put(~"$hint", Embedded(~speci));

        assert_eq!(cursor.query_spec, spec);
    }
    #[test]
    fn test_add_index_str() {
        let hint = ~"{\"foo\": 1}";
        let mut cursor = Cursor::new(BsonDocument::new(), None, 0i64, 0i32, 10i32, ~[]);
        cursor.hint(SpecNotation(hint));

        let mut spec = BsonDocument::new();
        let mut speci = BsonDocument::new();
        speci.put(~"foo", Double(1f64));
        spec.put(~"$hint", Embedded(~speci));

        assert_eq!(cursor.query_spec, spec);
    }    */
}
